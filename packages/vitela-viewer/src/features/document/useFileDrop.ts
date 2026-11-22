// Lets the user drop a file on the editor — or anywhere on the window, when
// the scope takes drops there (the app). Reports whether a file drag is over
// a place that takes it (to show a drop hint) and hands over the drop. Drags
// elsewhere on a host page are left to the host, default and all.
//
// The listeners sit on the window even when only the editor takes drops: a
// drop that React handlers inside the editor claimed must be seen after
// them, and React listens at its root container, above the editor's root.
//
// While disabled (a document is opening or the password prompt is up) a
// dropped file is ignored, but the listeners stay: without them the browser
// would open the file itself and unload the app.
//
// A drop that something on the page already claimed (it called
// `preventDefault`, as Organize does to add the dropped PDFs instead) ends the
// drag here but opens nothing.

import { useEffect, useRef, useState } from 'react';
import { handlesOfDrop, type WritableFileHandle } from '../../platform/fileHandle';
import { carriesFiles, droppedFile } from './droppedFile';
import { WHOLE_DOCUMENT, type EditorScope } from './editorScope';

export function useFileDrop(
  onFile: (file: File, fileHandle?: WritableFileHandle) => void,
  enabled: boolean,
  scope: EditorScope = WHOLE_DOCUMENT,
): boolean {
  const [dragging, setDragging] = useState(false);
  const handler = useRef(onFile);
  const accepting = useRef(enabled);
  /** Counts drops, so a drop whose handle arrives late is dropped if a newer one came. */
  const latestDrop = useRef(0);
  useEffect(() => {
    handler.current = onFile;
    accepting.current = enabled;
    if (!enabled) setDragging(false);
  }, [onFile, enabled]);

  useEffect(() => {
    // dragenter/dragleave fire for every child crossed; count them.
    let depth = 0;
    // Only drags over a place that takes drops count. Moving between two
    // elements fires dragenter on the new one before dragleave on the old,
    // so leaving the editor for the host brings the count back to zero.
    const carries = (event: DragEvent) => carriesFiles(event.dataTransfer ? [...event.dataTransfer.types] : undefined);
    const isFileDrag = (event: DragEvent) => scope.takesDrop(event.target) && carries(event);
    // An element entered inside the editor and then re-rendered away is no
    // longer inside anything; its dragleave must still count, or the hint sticks.
    const detached = (target: EventTarget | null) => target instanceof Node && !target.isConnected;

    const enter = (event: DragEvent) => {
      if (!isFileDrag(event)) return;
      depth++;
      if (accepting.current) setDragging(true);
    };
    const leave = (event: DragEvent) => {
      if (!isFileDrag(event) && !(detached(event.target) && carries(event))) return;
      depth = Math.max(0, depth - 1);
      if (depth === 0) setDragging(false);
    };
    const over = (event: DragEvent) => {
      if (!isFileDrag(event)) return;
      // Without this the browser opens the file itself instead of dropping.
      event.preventDefault();
      if (event.dataTransfer) event.dataTransfer.dropEffect = accepting.current ? 'copy' : 'none';
    };
    const onDrop = (event: DragEvent) => {
      if (!isFileDrag(event)) return;
      const claimed = event.defaultPrevented;
      event.preventDefault();
      depth = 0;
      setDragging(false);
      if (claimed || !accepting.current) return;
      const sequence = ++latestDrop.current;
      const files = [...(event.dataTransfer?.files ?? [])];
      const file = droppedFile(files);
      if (!file) return;
      // Must start now: the browser invalidates the drop's items afterwards.
      const handles = handlesOfDrop(event.dataTransfer?.items ?? []);
      const handle = handles?.[files.indexOf(file)];
      if (!handle) handler.current(file);
      else {
        void handle.then((fileHandle) => {
          // The handle takes a moment: a newer drop, or an open that started
          // meanwhile, has priority over this one.
          if (sequence !== latestDrop.current || !accepting.current) return;
          // A handle for another file would make Save overwrite the wrong one.
          handler.current(file, fileHandle?.name === file.name ? fileHandle : undefined);
        });
      }
    };

    window.addEventListener('dragenter', enter);
    window.addEventListener('dragleave', leave);
    window.addEventListener('dragover', over);
    window.addEventListener('drop', onDrop);
    return () => {
      window.removeEventListener('dragenter', enter);
      window.removeEventListener('dragleave', leave);
      window.removeEventListener('dragover', over);
      window.removeEventListener('drop', onDrop);
    };
  }, [scope]);

  return dragging;
}
