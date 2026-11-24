// The dialog of a text box's text, new or already on the page, with the
// core's layout previewed under it in the box's own frame. Container for the
// preview (it asks the engine); the rest is `TextDialog`.

import { useState } from 'react';
import type { Engine } from '@vitela/core';
import type { Size } from './drawing';
import { FreeTextPreview } from './FreeTextPreview';
import { TextDialog } from './TextDialog';
import { useFreeTextPreview } from './useFreeTextPreview';

interface FreeTextDialogProps {
  readonly engine: Engine;
  readonly title: string;
  readonly submit: string;
  readonly initialText?: string;
  /** The box's own frame (sides swapped on a quarter-turned page). */
  readonly frame: Size;
  readonly busy: boolean;
  /** The core's refusal of the text sent. */
  readonly error: string | null;
  readonly onChange: () => void;
  readonly onSubmit: (text: string) => void;
  readonly onCancel: () => void;
}

export function FreeTextDialog(props: FreeTextDialogProps) {
  const { engine, title, submit, initialText = '', frame, busy, error, onChange, onSubmit, onCancel } = props;
  const [text, setText] = useState(initialText);
  const preview = useFreeTextPreview(engine, text, frame.width, frame.height);
  return (
    <TextDialog
      title={title}
      label="Text"
      submit={submit}
      initialText={initialText}
      busy={busy}
      // The core's answer to what was sent first; until then, what the preview found.
      error={error ?? preview.error}
      onChange={(next) => {
        setText(next);
        onChange();
      }}
      onSubmit={onSubmit}
      onCancel={onCancel}
    >
      <FreeTextPreview frame={frame} layout={preview.layout} />
    </TextDialog>
  );
}
