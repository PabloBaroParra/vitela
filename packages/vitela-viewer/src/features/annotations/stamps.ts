// Image stamps: which files can be one, what their bytes are, and the edit
// that adds one. Pure.
//
// The core decides the stamp's box (`stampPlacement`) and decodes the image
// itself; the shell only checks, before any of that, that the bytes are a
// PNG or a JPEG, and keeps them as a data URL to draw the stamp with — the
// engine hands no image back, and a data URL needs no request (zero-network)
// and no clean-up.

import type { AnnotationEdit, PageRect } from '@vitela/core';
import { isPdf } from '../document/droppedFile';

/** The image types a stamp can be (the core decodes these two). */
export type StampImageType = 'image/png' | 'image/jpeg';

/** The picker's filter: by type, and by name for files the browser types as nothing. */
export const STAMP_ACCEPT = 'image/png,image/jpeg,.png,.jpg,.jpeg';

/**
 * The largest image a stamp can be, in bytes, checked on the file's size
 * before it is read. The shell holds such an image three times over (its
 * bytes, its `data:` URL at ~4/3, and the engine's copy) for the session:
 * 20 MB covers any logo, signature or photo worth stamping, and keeps a
 * mis-picked camera RAW or scan from stalling the tab.
 */
export const MAX_STAMP_BYTES = 20 * 1024 * 1024;

const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
const JPEG_SIGNATURE = [0xff, 0xd8, 0xff];
const STAMP_TYPES: readonly string[] = ['image/png', 'image/jpeg'];
const STAMP_NAME = /\.(png|jpe?g)$/i;
/** Bytes per `String.fromCharCode` call: well under any engine's argument limit. */
const CHUNK = 0x8000;

const startsWith = (bytes: Uint8Array, signature: readonly number[]) =>
  bytes.length >= signature.length && signature.every((byte, index) => bytes[index] === byte);

/** What `bytes` are by their first bytes (whatever the file's name or type says), or `null`. */
export function imageType(bytes: Uint8Array): StampImageType | null {
  if (startsWith(bytes, PNG_SIGNATURE)) return 'image/png';
  if (startsWith(bytes, JPEG_SIGNATURE)) return 'image/jpeg';
  return null;
}

/** `bytes` as a `data:` URL of `type`. */
export function dataUrl(bytes: Uint8Array, type: StampImageType): string {
  let binary = '';
  for (let start = 0; start < bytes.length; start += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(start, start + CHUNK));
  }
  return `data:${type};base64,${btoa(binary)}`;
}

/** Whether `file` claims to be a PNG or JPEG, by its type or its name (its bytes decide later). */
export function isStampFile(file: File): boolean {
  return STAMP_TYPES.includes(file.type) || STAMP_NAME.test(file.name);
}

/**
 * The image a drop onto a page stamps: the first PNG or JPEG among `files`,
 * unless a PDF came with them — that drop is the workspace's, to open it.
 */
export function stampFileOf(files: readonly File[]): File | null {
  if (files.some(isPdf)) return null;
  return files.find(isStampFile) ?? null;
}

/** The edit that adds `image` as a stamp in `rect`, the box `stampPlacement` gave it. */
export function stampEdit(page: number, rect: PageRect, image: Uint8Array): AnnotationEdit {
  return { kind: 'addStamp', page, rect, image };
}
