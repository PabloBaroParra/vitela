// Public entry of @vitela/viewer: the editor UI on top of @vitela/core.
// Deliberately minimal for now; T-247 designs the embedding API.

export { DocumentWorkspace, type DocumentScreen } from './features/document/DocumentWorkspace';
export type { EventScope } from './features/document/editorScope';
export { useEngineStatus, type EngineStatus } from './engine/useEngineStatus';
export { StatusLine } from './ui/StatusLine';
