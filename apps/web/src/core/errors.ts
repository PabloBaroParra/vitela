// Engine failures as the UI sees them. The core names every failure kind
// (core/pdf-wasm/src/error.rs) and the worker forwards that name, so the UI
// branches on `name` and shows `message` as is.

/** The failure kinds the UI branches on. Others are shown generically. */
export const PASSWORD_REQUIRED = 'PasswordRequired';
export const WRONG_PASSWORD = 'WrongPassword';
export const TEXT_EXTRACTION_DENIED = 'TextExtractionDenied';

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
