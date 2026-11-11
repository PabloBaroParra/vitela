//! Page rasterization: whole pages, clipped regions and tile batches, plus
//! the raster-size ceiling every one of them enforces.
//!
//! The `*_bitmaps` functions return owned [`Bitmap`]s and are what the
//! threadless wasm32 front end calls; the `*_job` wrappers register those
//! bitmaps in the state's [`crate::bitmap::BitmapRegistry`] for the native
//! actor, whose callers hold a release-on-drop [`BitmapHandle`] instead.

use pdfium_render::prelude::{PdfBitmap, PdfBitmapFormat, PdfPageRenderRotation, PdfRenderConfig};

use super::{document, page};
use crate::bitmap::Bitmap;
#[cfg(not(target_arch = "wasm32"))]
use crate::bitmap::BitmapHandle;
use crate::error::RenderError;
use crate::inversion::invert_rgba_in_place;
use crate::options::{Rect, RenderOptions, Tile};
use crate::state::{DocHandle, PdfiumState};

/// Hard ceiling on a single rasterized page, enforced for every shell that
/// reaches the renderer. A degenerate MediaBox (e.g. 1pt wide × 50,000pt tall)
/// scaled to a fit-to-width DPI would otherwise ask pdfium to allocate an
/// unbounded bitmap on the shared actor thread, freezing the pipeline or
/// exhausting memory. `MAX_RASTER_PIXELS` at 4 bytes/pixel also caps the
/// allocation at 128 MiB.
const MAX_RASTER_DIMENSION: i32 = 16_384;
const MAX_RASTER_PIXELS: u64 = 32 * 1024 * 1024;

fn ensure_raster_within_limits(width: i32, height: i32) -> Result<(), RenderError> {
    let pixels = (width.max(0) as u64).saturating_mul(height.max(0) as u64);
    if width > MAX_RASTER_DIMENSION || height > MAX_RASTER_DIMENSION || pixels > MAX_RASTER_PIXELS {
        return Err(RenderError::RenderFailed(format!(
            "requested raster {width}x{height} exceeds the maximum safe size"
        )));
    }
    Ok(())
}

/// The pixel size a whole page of `width_pt` x `height_pt` rasterizes to at
/// `dpi` — the one sizing rule [`render_page_bitmap`] applies.
fn full_page_raster_size(width_pt: f32, height_pt: f32, dpi: u32) -> (i32, i32) {
    let scale = dpi as f32 / 72.0;
    (
        (width_pt * scale).round().max(1.0) as i32,
        (height_pt * scale).round().max(1.0) as i32,
    )
}

/// Whether a whole page of `width_pt` x `height_pt` can be rendered at `dpi`
/// without hitting the raster ceiling every render enforces.
///
/// Asked *before* a long job — an export of many pages — so a page that would
/// be refused is named up front rather than after the pages before it have
/// already been written. The same sizing and the same limits as the render
/// itself, so the answer cannot drift from what the render would do.
pub fn full_page_raster_fits(width_pt: f32, height_pt: f32, dpi: u32) -> bool {
    let (width, height) = full_page_raster_size(width_pt, height_pt, dpi);
    ensure_raster_within_limits(width, height).is_ok()
}

/// Renders one page (or a region of it, in PDF points) at `dpi` into an
/// owned RGBA bitmap.
pub(crate) fn render_page_bitmap(
    state: &PdfiumState,
    doc: DocHandle,
    page_index: u32,
    dpi: u32,
    region: Option<Rect>,
    options: RenderOptions,
) -> Result<Bitmap, RenderError> {
    let document = document(state, doc)?;
    let page = page(document, page_index)?;

    let scale = dpi as f32 / 72.0;

    let config = match region {
        None => {
            let (target_width, target_height) =
                full_page_raster_size(page.width().value, page.height().value, dpi);
            ensure_raster_within_limits(target_width, target_height)?;
            PdfRenderConfig::new()
                .set_target_width(target_width)
                .set_target_height(target_height)
                .rotate(PdfPageRenderRotation::None, false)
        }
        Some(rect) => {
            let (full_width, full_height) =
                full_page_raster_size(page.width().value, page.height().value, dpi);
            ensure_raster_within_limits(full_width, full_height)?;
            let clip_left = (rect.left * scale).round() as i32;
            let clip_top = (rect.top * scale).round() as i32;
            let clip_right = (rect.right * scale).round() as i32;
            let clip_bottom = (rect.bottom * scale).round() as i32;
            PdfRenderConfig::new()
                .set_target_width(full_width)
                .set_target_height(full_height)
                .clip(clip_left, clip_top, clip_right, clip_bottom)
        }
    };

    let bitmap = page
        .render_with_config(&config)
        .map_err(|e| RenderError::RenderFailed(e.to_string()))?;

    let width = bitmap.width() as u32;
    let height = bitmap.height() as u32;
    let mut pixels = bitmap.as_rgba_bytes();

    if options.invert_content_colors {
        invert_rgba_in_place(&mut pixels);
    }

    let stride = width * 4;
    Ok(Bitmap {
        width,
        height,
        stride,
        pixels,
    })
}

/// [`render_page_bitmap`], registered for a release-on-drop handle.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn render_page_job(
    state: &mut PdfiumState,
    doc: DocHandle,
    page_index: u32,
    dpi: u32,
    region: Option<Rect>,
    options: RenderOptions,
) -> Result<BitmapHandle, RenderError> {
    let bitmap = render_page_bitmap(state, doc, page_index, dpi, region, options)?;
    Ok(state.bitmaps.insert(bitmap))
}

/// Renders several tiles of one page, loading the page once, into owned
/// RGBA bitmaps in the order given. Fails as a unit: one bad tile fails the
/// whole batch.
pub(crate) fn render_page_tiles_bitmaps(
    state: &PdfiumState,
    doc: DocHandle,
    page_index: u32,
    dpi: u32,
    tiles: &[Tile],
    options: RenderOptions,
) -> Result<Vec<Bitmap>, RenderError> {
    let document = document(state, doc)?;
    // Loaded once and held across every tile: this is the whole point of the
    // batch. `pages().get()` is an `FPDF_LoadPage`, which parses the content
    // stream — repeating it per tile is what made deep zoom crawl.
    let page = page(document, page_index)?;
    let scale = dpi as f32 / 72.0;
    let full_width = (page.width().value * scale).round().max(1.0) as i32;
    let full_height = (page.height().value * scale).round().max(1.0) as i32;

    let mut bitmaps = Vec::with_capacity(tiles.len());
    for tile in tiles {
        let (left, top, tile_width, tile_height) = tile_bounds(*tile, full_width, full_height)?;
        ensure_raster_within_limits(tile_width, tile_height)?;

        // Pdfium-render's origin contract is explicitly stitchable: render the
        // full page transform into the tile-sized destination at a negative
        // offset.
        let config = PdfRenderConfig::new()
            .set_fixed_size(full_width, full_height)
            .set_origin(-left, -top);
        let mut bitmap = PdfBitmap::empty(tile_width, tile_height, PdfBitmapFormat::default())
            .map_err(|error| RenderError::RenderFailed(error.to_string()))?;
        page.render_into_bitmap_with_config(&mut bitmap, &config)
            .map_err(|error| RenderError::RenderFailed(error.to_string()))?;

        let width = bitmap.width() as u32;
        let height = bitmap.height() as u32;
        let mut pixels = bitmap.as_rgba_bytes();
        if options.invert_content_colors {
            invert_rgba_in_place(&mut pixels);
        }
        bitmaps.push(Bitmap {
            width,
            height,
            stride: width * 4,
            pixels,
        });
    }

    Ok(bitmaps)
}

/// [`render_page_tiles_bitmaps`], each tile registered for a release-on-drop
/// handle.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn render_page_tiles_job(
    state: &mut PdfiumState,
    doc: DocHandle,
    page_index: u32,
    dpi: u32,
    tiles: &[Tile],
    options: RenderOptions,
) -> Result<Vec<BitmapHandle>, RenderError> {
    let bitmaps = render_page_tiles_bitmaps(state, doc, page_index, dpi, tiles, options)?;
    Ok(bitmaps
        .into_iter()
        .map(|bitmap| state.bitmaps.insert(bitmap))
        .collect())
}

/// Validates one tile against the page's raster size, returning it as
/// `(left, top, width, height)` in signed output pixels.
fn tile_bounds(
    tile: Tile,
    full_width: i32,
    full_height: i32,
) -> Result<(i32, i32, i32, i32), RenderError> {
    let tile_width = i32::try_from(tile.width)
        .map_err(|_| RenderError::RenderFailed("tile width exceeds i32".to_string()))?;
    let tile_height = i32::try_from(tile.height)
        .map_err(|_| RenderError::RenderFailed("tile height exceeds i32".to_string()))?;
    let left = i32::try_from(tile.left)
        .map_err(|_| RenderError::RenderFailed("tile left exceeds i32".to_string()))?;
    let top = i32::try_from(tile.top)
        .map_err(|_| RenderError::RenderFailed("tile top exceeds i32".to_string()))?;
    if tile_width <= 0
        || tile_height <= 0
        || left < 0
        || top < 0
        || left.saturating_add(tile_width) > full_width
        || top.saturating_add(tile_height) > full_height
    {
        return Err(RenderError::RenderFailed(
            "tile lies outside the rendered page".to_string(),
        ));
    }

    Ok((left, top, tile_width, tile_height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_limit_accepts_a_normal_page() {
        assert!(ensure_raster_within_limits(1_240, 1_754).is_ok());
    }

    #[test]
    fn raster_limit_rejects_a_degenerate_page() {
        // A 1pt × 50,000pt MediaBox at a fit-to-width DPI.
        assert!(matches!(
            ensure_raster_within_limits(600, 30_000_000),
            Err(RenderError::RenderFailed(_))
        ));
    }

    /// US Letter at 400 DPI is 3400x4400 (15 Mpx); at 600 it is 5100x6600
    /// (33.7 Mpx), just over `MAX_RASTER_PIXELS`. A shell's export ceiling is
    /// chosen against exactly this line.
    #[test]
    fn a_letter_page_fits_at_400_dpi_but_not_at_600() {
        assert!(full_page_raster_fits(612.0, 792.0, 400));
        assert!(!full_page_raster_fits(612.0, 792.0, 600));
    }

    #[test]
    fn a_degenerate_page_does_not_fit_at_any_ordinary_dpi() {
        assert!(!full_page_raster_fits(1.0, 50_000.0, 72));
    }

    #[test]
    fn tile_raster_limit_accepts_a_one_megapixel_tile() {
        assert!(ensure_raster_within_limits(1024, 1024).is_ok());
    }

    #[test]
    fn tile_bounds_accepts_a_grid_cell_clipped_by_the_page() {
        let tile = Tile {
            left: 1024,
            top: 2048,
            width: 300,
            height: 1024,
        };
        assert_eq!(
            tile_bounds(tile, 1324, 4096).unwrap(),
            (1024, 2048, 300, 1024)
        );
    }

    #[test]
    fn tile_bounds_rejects_a_tile_past_the_page_edge() {
        let tile = Tile {
            left: 1024,
            top: 0,
            width: 1024,
            height: 512,
        };
        assert!(tile_bounds(tile, 1500, 512).is_err());
    }

    #[test]
    fn tile_bounds_rejects_an_empty_tile() {
        let tile = Tile {
            left: 0,
            top: 0,
            width: 0,
            height: 512,
        };
        assert!(tile_bounds(tile, 1024, 1024).is_err());
    }
}
