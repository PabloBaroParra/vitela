// Export pages as images: which pages, PNG or JPEG, and at what resolution
// — the native dialogs' three questions. It stays open on a refused choice
// and says why; a document that withholds extraction says so and offers
// only Cancel. Presentational: the draft and the answers are the caller's.

import { useId, useRef } from 'react';
import type { ImageExportFormat } from '@vitela/core';
import { ModalShell } from '../annotations/ModalShell';
import { MAX_EXPORT_DPI, MIN_EXPORT_DPI, formatLabel, type ExportDraft, type ExportPages } from './exportText';

const PAGES: readonly { readonly value: ExportPages; readonly label: string }[] = [
  { value: 'all', label: 'All pages' },
  { value: 'current', label: 'Current page' },
  { value: 'custom', label: 'Custom range' },
];

interface ExportImagesDialogProps {
  readonly draft: ExportDraft;
  /** The formats the core offers, in its order. */
  readonly formats: readonly ImageExportFormat[];
  readonly allowed: boolean;
  /** Why the last Export was refused, or why none is possible; `null` when nothing was. */
  readonly message: string | null;
  readonly checking: boolean;
  readonly onChange: (draft: ExportDraft) => void;
  readonly onExport: () => void;
  readonly onCancel: () => void;
}

export function ExportImagesDialog({ draft, formats, allowed, message, checking, onChange, onExport, onCancel }: ExportImagesDialogProps) {
  const chosen = useRef<HTMLInputElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const pagesName = useId();
  const formatName = useId();
  const dpiId = useId();
  const disabled = !allowed;

  return (
    <ModalShell
      title="Export pages as images"
      className="export-dialog"
      busy={checking}
      field={allowed ? chosen : cancel}
      onCancel={onCancel}
    >
      {(heading) => (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (allowed && !checking) onExport();
          }}
        >
          {heading}
          {message !== null && (
            <p className="export-dialog__refusal" role="alert">
              {message}
            </p>
          )}
          <fieldset className="export-dialog__group">
            <legend className="export-dialog__legend">Pages</legend>
            {PAGES.map(({ value, label }) => (
              <label key={value} className="export-dialog__choice">
                <input
                  ref={value === draft.pages ? chosen : undefined}
                  type="radio"
                  name={pagesName}
                  value={value}
                  checked={draft.pages === value}
                  disabled={disabled}
                  onChange={() => onChange({ ...draft, pages: value })}
                />
                {label}
              </label>
            ))}
            <input
              type="text"
              className="export-dialog__range"
              aria-label="Page range"
              placeholder="e.g. 1-3,7"
              value={draft.customRange}
              disabled={disabled}
              // Typing a range means a range: no need to pick the choice first.
              onChange={(event) => onChange({ ...draft, pages: 'custom', customRange: event.target.value })}
            />
          </fieldset>
          <fieldset className="export-dialog__group export-dialog__group--row">
            <legend className="export-dialog__legend">Format</legend>
            {formats.map((format) => (
              <label key={format} className="export-dialog__choice">
                <input
                  type="radio"
                  name={formatName}
                  value={format}
                  checked={draft.format === format}
                  disabled={disabled}
                  onChange={() => onChange({ ...draft, format })}
                />
                {formatLabel(format)}
              </label>
            ))}
          </fieldset>
          <label className="export-dialog__legend" htmlFor={dpiId}>
            {`Resolution (${MIN_EXPORT_DPI}-${MAX_EXPORT_DPI} DPI)`}
          </label>
          {/*
            Text, not `type="number"`: a number field's own range check would
            stop the form with the browser's bubble before the dialog can say
            why. Digits only, three at most, as the native boxes take them.
          */}
          <input
            id={dpiId}
            type="text"
            className="export-dialog__dpi"
            inputMode="numeric"
            autoComplete="off"
            value={draft.dpi}
            disabled={disabled}
            onChange={(event) => onChange({ ...draft, dpi: event.target.value.replace(/\D/g, '').slice(0, 3) })}
          />
          <div className="dialog__actions">
            <button ref={cancel} type="button" onClick={onCancel} disabled={checking}>
              Cancel
            </button>
            <button type="submit" className="button--primary" disabled={disabled || checking}>
              Export
            </button>
          </div>
        </form>
      )}
    </ModalShell>
  );
}
