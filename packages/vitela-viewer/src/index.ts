// Public entry of @vitela/viewer: the editor UI on top of @vitela/core.
//
// For a host page: `<VitelaViewer>` (React) or `mountViewer` (no React of
// its own). `DocumentWorkspace`, `useEngineStatus` and `StatusLine` are the
// lower-level pieces apps/web builds its own shell from (its landing page and
// tool routes around the editor).

export { VitelaViewer, type VitelaViewerHandle, type VitelaViewerProps, type ViewerEngine } from './embed/VitelaViewer';
export { mountViewer, type MountedViewer, type MountViewerOptions } from './embed/mountViewer';
export type { ViewerSource } from './embed/source';
export type { SaveCallback } from './features/save/callbackDelivery';
export { DocumentWorkspace, type DocumentScreen } from './features/document/DocumentWorkspace';
export type { EventScope } from './features/document/editorScope';
export { useEngineStatus, type EngineStatus } from './engine/useEngineStatus';
export { StatusLine } from './ui/StatusLine';
