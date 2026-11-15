// Lets the user drop a file anywhere on the window. Reports whether a file
// drag is over the window (to show a drop hint) and hands over the drop.
//
// While disabled (a document is opening or the password prompt is up) a
// dropped file is ignored, but the listeners stay: without them the browser
// would open the file itself and unload the app.

import { useEffect, useRef, useState } from 'react';
import { handlesOfDrop, type WritableFileHandle } from '../../platform/fileHandle';
import { carriesFiles, droppedFile } from './droppedFile';

export function useFileDrop(onFile: (file: File, fileHandle?: WritableFileHandle) => void, enabled: boolean): boolean {
  const [dragging, setDragging] = useState(false);
  const handler = useRef(onFile);
  const accepting = useRef(enabled);
  useEffect(() => {
    handler.current = onFile;
    accepting.current = enabled;
    if (!enabled) setDragging(false);
  }, [onFile, enabled]);

  useEffect(() => {
    // dragenter/dragleave fire for every child crossed; count them.
    let depth = 0;
    const isFileDrag = (event: DragEvent) => carriesFiles(event.dataTransfer ? [...event.dataTransfer.types] : undefined);

    const enter = (event: DragEvent) => {
      if (!isFileDrag(event)) return;
      depth++;
      if (accepting.current) setDragging(true);
    };
    const leave = (event: DragEvent) => {
      if (!isFileDrag(event)) return;
      depth = Math.max(0, depth - 1);
      if (depth === 0) setDragging(false);
    };
    const over = (event: DragEvent) => {
      if (!isFileDrag(event)) return;
      // Without this the browser opens the file itself instead of dropping.
      event.preventDefault();
      if (event.dataTransfer) event.dataTransfer.dropEffect = accepting.current ? 'copy' : 'none';
    };
    const drop = (event: DragEvent) => {
      if (!isFileDrag(event)) return;
      event.preventDefault();
      depth = 0;
      setDragging(false);
      if (!accepting.current) return;
      const files = [...(event.dataTransfer?.files ?? [])];
      const file = droppedFile(files);
      if (!file) return;
      // Must start now: the browser invalidates the drop's items afterwards.
      const handles = handlesOfDrop(event.dataTransfer?.items ?? []);
      const handle = handles?.[files.indexOf(file)];
      if (!handle) handler.current(file);
      else void handle.then((fileHandle) => handler.current(file, fileHandle ?? undefined));
    };

    window.addEventListener('dragenter', enter);
    window.addEventListener('dragleave', leave);
    window.addEventListener('dragover', over);
    window.addEventListener('drop', drop);
    return () => {
      window.removeEventListener('dragenter', enter);
      window.removeEventListener('dragleave', leave);
      window.removeEventListener('dragover', over);
      window.removeEventListener('drop', drop);
    };
  }, []);

  return dragging;
}
