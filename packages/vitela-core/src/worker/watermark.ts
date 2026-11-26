// Watermarks across the worker boundary: the `WatermarkEdit` becomes one of
// core/pdf-wasm's two stamp `EditCommand`s, and the core's `WatermarkLook`
// (a wasm object) becomes plain values.
//
// Every rule is the core's (core/pdf-wasm/src/model/watermark.rs): the gate,
// the pages resolved once, the mark checked before it is recorded, the
// starting looks. This only moves the values across.

import type { WatermarkDefaults, WatermarkEdit } from '../core/types.js';
import type { Owned } from './pageText.js';

/** core/pdf-wasm's watermark `EditCommand` constructors. */
export interface CoreWatermarkEdits {
  addTextWatermark(
    pages: Uint32Array,
    text: string,
    fontSize: number,
    opacity: number,
    rotationDegrees: number,
    behind: boolean,
  ): Owned;
  addPictureWatermark(
    pages: Uint32Array,
    image: Uint8Array,
    scale: number,
    opacity: number,
    rotationDegrees: number,
    behind: boolean,
  ): Owned;
}

/** core/pdf-wasm's `WatermarkLook`. */
export interface CoreWatermarkLook extends Owned, WatermarkDefaults {}

/** The core command for `edit`. */
export function watermarkCommand(edits: CoreWatermarkEdits, edit: WatermarkEdit): Owned {
  const pages = Uint32Array.from(edit.pages);
  const { mark, opacity, rotationDegrees, behind } = edit;
  return mark.kind === 'text'
    ? edits.addTextWatermark(pages, mark.text, mark.fontSize, opacity, rotationDegrees, behind)
    : edits.addPictureWatermark(pages, mark.image, mark.scale, opacity, rotationDegrees, behind);
}

/** Copies the core's looks into plain values and frees them. */
export function watermarkDefaults(look: CoreWatermarkLook): WatermarkDefaults {
  try {
    return {
      fontSize: look.fontSize,
      textOpacity: look.textOpacity,
      textRotationDegrees: look.textRotationDegrees,
      textBehind: look.textBehind,
      pictureScale: look.pictureScale,
      pictureOpacity: look.pictureOpacity,
      pictureRotationDegrees: look.pictureRotationDegrees,
      pictureBehind: look.pictureBehind,
    };
  } finally {
    look.free();
  }
}
