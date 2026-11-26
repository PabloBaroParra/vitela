// Engine failures as the UI sees them. The core names every failure kind
// (core/pdf-wasm/src/error.rs) and the worker forwards that name, so the UI
// branches on `name` and shows `message` as is.

/** The failure kinds the UI branches on. Others are shown generically. */
export const PASSWORD_REQUIRED = 'PasswordRequired';
export const WRONG_PASSWORD = 'WrongPassword';
export const TEXT_EXTRACTION_DENIED = 'TextExtractionDenied';
export const SIGNATURES_WOULD_BE_INVALIDATED = 'SignaturesWouldBeInvalidated';
export const EDIT_NOT_PERMITTED = 'EditNotPermitted';
/** Free text holds a character the box's font (Helvetica) cannot show; the message names it. */
export const ENCODING_GAP = 'EncodingGap';
/** No annotation of this session has that id (removed, or undone away). */
export const ANNOTATION_NOT_FOUND = 'AnnotationNotFound';
/** A stamp's bytes are not a PNG or JPEG the engine can decode. */
export const INVALID_IMAGE = 'InvalidImage';
/** No form field of the document has that id. */
export const FORM_FIELD_NOT_FOUND = 'FormFieldNotFound';

/** A failure reported by the engine, carrying the core's kind as `name`. */
export class EngineError extends Error {
  constructor(name: string, message: string) {
    super(message);
    this.name = name;
  }
}

/** The kind of a failure, or `null` for anything that is not an `Error`. */
export function errorKind(error: unknown): string | null {
  return error instanceof Error ? error.name : null;
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
