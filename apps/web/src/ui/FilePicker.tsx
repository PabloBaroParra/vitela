// A button-styled file input. Presentational: it only reports the pick.
//
// Given `pick` (the browser's own file picker, which can hand back a handle
// to save over the file), a click opens that instead of the native input
// dialog. The input stays in the DOM either way: it is the keyboard/AT
// target and the fallback.

import { useId, useRef, type ChangeEvent, type MouseEvent } from 'react';
import type { PickedFile, WritableFileHandle } from '../platform/fileHandle';

interface FilePickerProps {
  readonly label: string;
  readonly accept: string;
  readonly disabled?: boolean;
  readonly onPick: (file: File, handle?: WritableFileHandle) => void;
  /** The browser's picker; resolves `null` when the user dismisses it. */
  readonly pick?: () => Promise<PickedFile | null>;
}

export function FilePicker({ label, accept, disabled = false, onPick, pick }: FilePickerProps) {
  const id = useId();
  /** Set once the browser picker failed: from then on the native dialog opens. */
  const native = useRef(false);
  const handleChange = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    // Reset so picking the same file again still fires a change.
    event.target.value = '';
    if (file) onPick(file);
  };
  const handleClick = (event: MouseEvent<HTMLInputElement>) => {
    if (!pick || native.current) return;
    event.preventDefault();
    pick().then(
      (picked) => picked && onPick(picked.file, picked.handle),
      // The browser picker failed (blocked, say): the next click opens the
      // native dialog. Clicking again here would lack the user's gesture.
      () => void (native.current = true),
    );
  };
  return (
    <label className="file-picker" htmlFor={id} aria-disabled={disabled}>
      {label}
      <input id={id} type="file" accept={accept} disabled={disabled} onChange={handleChange} onClick={handleClick} />
    </label>
  );
}
