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
  const input = useRef<HTMLInputElement>(null);
  /** Lets the one click that falls back to the native dialog through. */
  const letThrough = useRef(false);
  const handleChange = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    // Reset so picking the same file again still fires a change.
    event.target.value = '';
    if (file) onPick(file);
  };
  const handleClick = (event: MouseEvent<HTMLInputElement>) => {
    if (!pick) return;
    if (letThrough.current) {
      letThrough.current = false;
      return;
    }
    event.preventDefault();
    pick().then(
      (picked) => picked && onPick(picked.file, picked.handle),
      (error: unknown) => {
        // The user dismissed the picker: nothing to do.
        if (error instanceof DOMException && error.name === 'AbortError') return;
        // The picker failed (blocked, say): open the native dialog for this
        // click only. The user's activation outlives the failed call, and the
        // picker stays in use for the next click.
        letThrough.current = true;
        input.current?.click();
        // The click is dispatched synchronously; never leave the pass armed.
        letThrough.current = false;
      },
    );
  };
  return (
    <label className="file-picker" htmlFor={id} aria-disabled={disabled}>
      {label}
      <input ref={input} id={id} type="file" accept={accept} disabled={disabled} onChange={handleChange} onClick={handleClick} />
    </label>
  );
}
