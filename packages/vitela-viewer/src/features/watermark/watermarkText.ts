// Add watermark's draft and words: what the dialog shows, the ranges its
// sliders take, the sentence for a mark that cannot be stamped, the edit a
// draft becomes and the status line after it. Pure.
//
// The ranges and the words are the native shells' (Android's `Watermark.kt`,
// which shares GTK's and Windows' ranges); the starting look is the core's
// (`watermarkDefaults`), never a copy of its constants.

import type { WatermarkDefaults, WatermarkEdit } from '@vitela/core';

export type WatermarkKind = 'text' | 'picture';
export type WatermarkPages = 'all' | 'current' | 'custom';

/** A PNG or JPEG exactly as the reader picked it: decoding is the core's job. */
export interface WatermarkPicture {
  readonly name: string;
  readonly bytes: Uint8Array;
}

/**
 * What the dialog shows. Both kinds share the pages, opacity, angle and
 * placement; the text's size is in points, the picture's in percent of the
 * page. Opacity is asked in percent, the way every image editor asks it.
 */
export interface WatermarkDraft {
  readonly kind: WatermarkKind;
  readonly text: string;
  readonly picture: WatermarkPicture | null;
  readonly pages: WatermarkPages;
  /** The one-based range typed for `custom`, such as `"1-3,7"`. */
  readonly customRange: string;
  readonly fontSize: number;
  readonly scalePercent: number;
  readonly opacityPercent: number;
  /** Counter-clockwise, as the reader sees the page. */
  readonly rotationDegrees: number;
  /** Painted behind the page's content instead of over it. */
  readonly behind: boolean;
}

export interface SliderRange {
  readonly min: number;
  readonly max: number;
}

/**
 * The sliders' ranges, the native shells' own: the size floor keeps the
 * stamp legible and its ceiling is past what fits across portrait A4 at 45°;
 * the opacity floor is 5, not 1, because 1% is a stamp nobody will find.
 */
export const FONT_SIZE_RANGE: SliderRange = { min: 8, max: 200 };
export const OPACITY_PERCENT_RANGE: SliderRange = { min: 5, max: 100 };
/** A picture's share of the page: 100 touches two opposite edges. */
export const SCALE_PERCENT_RANGE: SliderRange = { min: 5, max: 100 };
export const ROTATION_RANGE: SliderRange = { min: -180, max: 180 };

export const DEFAULT_WATERMARK_TEXT = 'DRAFT';

export const TEXT_MISSING = 'Type the text to stamp on the pages.';
export const PICTURE_MISSING = 'Choose a PNG or JPEG picture to stamp on the pages.';
export const RANGE_MISSING = 'Type which pages to stamp, for example 1-3,7.';
export const NO_PAGES = 'This document has no pages to stamp.';
export const PICTURE_UNREADABLE = 'That file could not be read as a PNG or JPEG picture. Choose another one.';
export const FONT_GAP_HINT = 'Use Latin letters, digits and punctuation.';

const clamp = (value: number, { min, max }: SliderRange) => Math.min(max, Math.max(min, value));
const percent = (fraction: number) => Math.round(fraction * 100);

/** A fresh draft: text, drawn the core's way, with the picture's size ready for a switch. */
export function draftOf(look: WatermarkDefaults): WatermarkDraft {
  return {
    kind: 'text',
    text: DEFAULT_WATERMARK_TEXT,
    picture: null,
    pages: 'all',
    customRange: '',
    fontSize: clamp(look.fontSize, FONT_SIZE_RANGE),
    scalePercent: clamp(percent(look.pictureScale), SCALE_PERCENT_RANGE),
    opacityPercent: clamp(percent(look.textOpacity), OPACITY_PERCENT_RANGE),
    rotationDegrees: clamp(look.textRotationDegrees, ROTATION_RANGE),
    behind: look.textBehind,
  };
}

/**
 * `draft` showing `kind`. Switching resets the angle and the placement to
 * the core's defaults for that kind, as the native dialogs do: a logo left
 * at text's 45° reads as a mistake, and a background picture moved over the
 * text hides the page. Everything else is kept.
 */
export function switchKind(draft: WatermarkDraft, kind: WatermarkKind, look: WatermarkDefaults): WatermarkDraft {
  if (kind === draft.kind) return draft;
  return kind === 'text'
    ? { ...draft, kind, rotationDegrees: clamp(look.textRotationDegrees, ROTATION_RANGE), behind: look.textBehind }
    : { ...draft, kind, rotationDegrees: clamp(look.pictureRotationDegrees, ROTATION_RANGE), behind: look.pictureBehind };
}

/**
 * Why `draft`'s mark cannot be stamped, or `null`. Text is refused only when
 * all blank (it is kept as typed: the stamp is centred on its own width), a
 * picture only when none was chosen; whether it decodes is the core's call.
 */
export function markRefusal(draft: WatermarkDraft): string | null {
  if (draft.kind === 'text') return draft.text.trim() === '' ? TEXT_MISSING : null;
  return draft.picture === null ? PICTURE_MISSING : null;
}

/** The edit `draft` records on the 0-based `pages`. Call only when `markRefusal` is `null`. */
export function watermarkEdit(draft: WatermarkDraft, pages: readonly number[]): WatermarkEdit {
  const look = {
    opacity: draft.opacityPercent / 100,
    rotationDegrees: draft.rotationDegrees,
    behind: draft.behind,
  };
  const mark: WatermarkEdit['mark'] =
    draft.kind === 'text' || draft.picture === null
      ? { kind: 'text', text: draft.text, fontSize: draft.fontSize }
      : { kind: 'picture', image: draft.picture.bytes, scale: draft.scalePercent / 100 };
  return { kind: 'addWatermark', pages, mark, ...look };
}

/** What the dialog says under Choose picture. */
export function pictureSummary(picture: WatermarkPicture | null): string {
  if (picture === null) return 'No picture chosen. PNG or JPEG.';
  return `${picture.name} (${Math.max(1, Math.round(picture.bytes.length / 1024))} KB).`;
}

/** What the status line says once the stamp is recorded. */
export function watermarkStatus(pageCount: number): string {
  return `Watermark added to ${pageCount} ${pageCount === 1 ? 'page' : 'pages'}.`;
}
