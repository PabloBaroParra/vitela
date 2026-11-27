// Compress PDF: one radio per preset the core offers, Balanced chosen first.
// No predicted saving: nothing can answer that without running. A signed
// file says so here and the button reads "Compress anyway", so confirming is
// the consent; a document that cannot be compressed says why and offers only
// Cancel. Presentational: the choice and the answers are the caller's.

import { useId, useRef } from 'react';
import type { CompressPreset } from '@vitela/core';
import { ModalShell } from '../annotations/ModalShell';
import { presetText } from './compressText';

interface CompressDialogProps {
  readonly presets: readonly CompressPreset[];
  readonly preset: CompressPreset;
  /** Why the document cannot be compressed, as a sentence; `null` when it can. */
  readonly refusal: string | null;
  readonly signaturesWillBreak: boolean;
  readonly onSelect: (preset: CompressPreset) => void;
  readonly onCompress: () => void;
  readonly onCancel: () => void;
}

export function CompressDialog({
  presets,
  preset,
  refusal,
  signaturesWillBreak,
  onSelect,
  onCompress,
  onCancel,
}: CompressDialogProps) {
  const chosen = useRef<HTMLInputElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const name = useId();
  const refused = refusal !== null;

  return (
    <ModalShell title="Compress PDF" className="compress-dialog" field={refused ? cancel : chosen} onCancel={onCancel}>
      {(heading) => (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!refused) onCompress();
          }}
        >
          {heading}
          {refused && (
            <p className="compress-dialog__refusal" role="alert">
              {refusal}
            </p>
          )}
          <p className="compress-dialog__detail">Writes a smaller copy. The open document stays as it is.</p>
          <fieldset className="compress-dialog__presets">
            <legend className="compress-dialog__legend">Compression</legend>
            {presets.map((option) => {
              const text = presetText(option);
              return (
                <label key={option} className="compress-dialog__preset">
                  <input
                    ref={option === preset ? chosen : undefined}
                    type="radio"
                    name={name}
                    value={option}
                    checked={option === preset}
                    disabled={refused}
                    onChange={() => onSelect(option)}
                  />
                  <span className="compress-dialog__preset-text">
                    <span className="compress-dialog__preset-name">{text.name}</span>
                    <span className="compress-dialog__preset-detail">{text.detail}</span>
                  </span>
                </label>
              );
            })}
          </fieldset>
          {signaturesWillBreak && (
            <p className="compress-dialog__signed">
              This document is signed. Compressing rewrites the file, so the compressed copy&apos;s digital signature will
              no longer verify.
            </p>
          )}
          <div className="dialog__actions">
            <button ref={cancel} type="button" onClick={onCancel}>
              Cancel
            </button>
            <button type="submit" className="button--primary" disabled={refused}>
              {signaturesWillBreak ? 'Compress anyway' : 'Compress'}
            </button>
          </div>
        </form>
      )}
    </ModalShell>
  );
}
