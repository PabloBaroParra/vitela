// The dialog of a note or text box waiting for its text: the note's plain
// text area, or the text box's with the core's layout previewed under it.
// Container for the preview (it asks the engine); the rest is `TextDialog`.

import { useState } from 'react';
import type { Engine } from '@vitela/core';
import { FreeTextPreview } from './FreeTextPreview';
import { TextDialog } from './TextDialog';
import { useFreeTextPreview } from './useFreeTextPreview';
import type { Draft } from './useDrawing';

interface DraftDialogProps {
  readonly engine: Engine;
  readonly draft: Draft;
  readonly onChange: () => void;
  readonly onSubmit: (text: string) => void;
  readonly onCancel: () => void;
}

export function DraftDialog({ engine, draft, onChange, onSubmit, onCancel }: DraftDialogProps) {
  if (draft.tool === 'note') {
    return (
      <TextDialog
        title="Add a note"
        label="Note"
        submit="Add note"
        busy={draft.busy}
        error={draft.error}
        onChange={onChange}
        onSubmit={onSubmit}
        onCancel={onCancel}
      />
    );
  }
  return <FreeTextDialog engine={engine} draft={draft} onChange={onChange} onSubmit={onSubmit} onCancel={onCancel} />;
}

function FreeTextDialog({ engine, draft, onChange, onSubmit, onCancel }: DraftDialogProps) {
  const [text, setText] = useState('');
  const preview = useFreeTextPreview(engine, text, draft.frame.width, draft.frame.height);
  return (
    <TextDialog
      title="Add a text box"
      label="Text"
      submit="Add text box"
      busy={draft.busy}
      // The core's answer to Add first; until then, what the preview found.
      error={draft.error ?? preview.error}
      onChange={(next) => {
        setText(next);
        onChange();
      }}
      onSubmit={onSubmit}
      onCancel={onCancel}
    >
      <FreeTextPreview frame={draft.frame} layout={preview.layout} />
    </TextDialog>
  );
}
