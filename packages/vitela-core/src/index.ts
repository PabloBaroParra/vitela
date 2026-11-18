// @vitela/core: Vitela's PDF engine (the Rust core as WASM) in a Web Worker,
// behind a typed, framework-agnostic client. Public entry: re-exports only.

export { createEngine, type EngineOptions } from './createEngine.js';
export { EngineClient } from './core/client.js';
export {
  EngineError,
  EDIT_NOT_PERMITTED,
  PASSWORD_REQUIRED,
  SIGNATURES_WOULD_BE_INVALIDATED,
  TEXT_EXTRACTION_DENIED,
  WRONG_PASSWORD,
  errorKind,
  errorMessage,
} from './core/errors.js';
export type {
  BlockSource,
  DocumentBlock,
  DocumentHandle,
  DocumentInfo,
  EditedDocument,
  Engine,
  ImportedDocument,
  OpenedDocument,
  PageEdit,
  PagePoint,
  PageRects,
  PageSize,
  PreparedImport,
  RenderedPage,
  SearchHit,
  TextSelection,
} from './core/types.js';
export type { MessagePortLike, RequestEnvelope, WorkerMessage } from './core/protocol.js';
