// A button-styled file input. Presentational: it only reports the pick.

import { useId, type ChangeEvent } from 'react';

interface FilePickerProps {
  readonly label: string;
  readonly accept: string;
  readonly disabled?: boolean;
  readonly onPick: (file: File) => void;
}

export function FilePicker({ label, accept, disabled = false, onPick }: FilePickerProps) {
  const id = useId();
  const handleChange = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    // Reset so picking the same file again still fires a change.
    event.target.value = '';
    if (file) onPick(file);
  };
  return (
    <label className="file-picker" htmlFor={id} aria-disabled={disabled}>
      {label}
      <input id={id} type="file" accept={accept} disabled={disabled} onChange={handleChange} />
    </label>
  );
}
