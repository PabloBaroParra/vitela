// The editor for a page that does not use React itself: mounts
// `<VitelaViewer>` into an element and hands back what a host needs to drive
// it. React and React DOM still have to be installed (peer dependencies); the
// host just never touches them.

import { flushSync } from 'react-dom';
import { createRoot } from 'react-dom/client';
import type { ViewerSource } from './source';
import { VitelaViewer, type VitelaViewerHandle, type VitelaViewerProps } from './VitelaViewer';

/** `<VitelaViewer>`'s props, as options. */
export type MountViewerOptions = Omit<VitelaViewerProps, 'ref'>;

export interface MountedViewer {
  /** Opens `source` (once the engine is ready); the editor shows how it went. */
  open(source: ViewerSource, name?: string): void;
  /** Saves the open document as the Save button does (to `onSave`, when given). */
  save(): void;
  /**
   * Unmounts the editor and empties the element: the open document is closed
   * and an engine the editor started is stopped (one the host gave keeps
   * running). Later calls on this handle do nothing.
   */
  destroy(): void;
}

export function mountViewer(element: Element, options: MountViewerOptions = {}): MountedViewer {
  const root = createRoot(element);
  let viewer: VitelaViewerHandle | null = null;
  let destroyed = false;
  // Rendered synchronously, so the handle works as soon as this returns.
  flushSync(() =>
    root.render(
      <VitelaViewer
        {...options}
        ref={(handle) => {
          viewer = handle;
        }}
      />,
    ),
  );
  return {
    open: (source, name) => {
      if (!destroyed) viewer?.open(source, name);
    },
    save: () => {
      if (!destroyed) viewer?.save();
    },
    destroy: () => {
      if (destroyed) return;
      destroyed = true;
      root.unmount();
      viewer = null;
    },
  };
}
