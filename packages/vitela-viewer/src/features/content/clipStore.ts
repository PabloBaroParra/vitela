// Where the content clipboard's clip lives (B26 T-262): on the session, not
// on the open document's view, as the native shells keep it (Android's
// `ViewerSession`, GTK's `ViewerState`): it outlives the page and the
// document it was copied from, so a clip copied in one PDF pastes into the
// next one opened. The workspace holds it above its viewers; a viewer
// mounted without one keeps its own.

import { createContext, useContext, useMemo, useState } from 'react';
import type { ContentClip } from '@vitela/core';

export interface ClipStore {
  /** The last clip copied or cut; `null` before the first. */
  readonly clip: ContentClip | null;
  readonly keep: (clip: ContentClip) => void;
}

/** The session's clip store, provided by the workspace. */
export const ContentClipContext = createContext<ClipStore | null>(null);

/** A clip store of the caller's own. */
export function useClipStore(): ClipStore {
  const [clip, keep] = useState<ContentClip | null>(null);
  return useMemo(() => ({ clip, keep }), [clip]);
}

/** The session's clip store, or one of the viewer's own outside a workspace. */
export function useSessionClip(): ClipStore {
  const shared = useContext(ContentClipContext);
  const own = useClipStore();
  return shared ?? own;
}
