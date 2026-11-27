// Add watermark: text or a picture, its look (size, opacity, angle, over or
// behind the page's content) and the pages — the native dialogs' questions,
// with Android's labels. It stays open on a refused Add and says why; a
// document that cannot take a stamp says so and offers only Close.
// Presentational: the draft and the answers are the caller's.

import { useId, useRef, type ReactNode } from 'react';
import { ModalShell } from '../annotations/ModalShell';
import { FilePicker } from '../../ui/FilePicker';
import {
  FONT_SIZE_RANGE,
  OPACITY_PERCENT_RANGE,
  ROTATION_RANGE,
  SCALE_PERCENT_RANGE,
  pictureSummary,
  type SliderRange,
  type WatermarkDraft,
  type WatermarkKind,
  type WatermarkPages,
} from './watermarkText';

const KINDS: readonly { readonly value: WatermarkKind; readonly label: string }[] = [
  { value: 'text', label: 'Text' },
  { value: 'picture', label: 'Picture' },
];

const PAGES: readonly { readonly value: WatermarkPages; readonly label: string }[] = [
  { value: 'all', label: 'All pages' },
  { value: 'current', label: 'Current page' },
  { value: 'custom', label: 'Custom range' },
];

interface WatermarkDialogProps {
  readonly draft: WatermarkDraft;
  /** Why no stamp can be added, as a sentence; `null` when one can. */
  readonly refusal: string | null;
  /** Why the last Add did not go in, as a sentence. */
  readonly error: string | null;
  /** The stamp is with the core: nothing can be changed or cancelled. */
  readonly running: boolean;
  readonly onEdit: (draft: WatermarkDraft) => void;
  readonly onSwitch: (kind: WatermarkKind) => void;
  readonly onPickPicture: (file: File) => void;
  readonly onAdd: () => void;
  readonly onCancel: () => void;
}

export function WatermarkDialog({
  draft,
  refusal,
  error,
  running,
  onEdit,
  onSwitch,
  onPickPicture,
  onAdd,
  onCancel,
}: WatermarkDialogProps) {
  const first = useRef<HTMLInputElement>(null);
  const close = useRef<HTMLButtonElement>(null);
  const kindName = useId();
  const layerName = useId();
  const pagesName = useId();
  const errorId = useId();
  const readOnly = refusal !== null;
  const disabled = readOnly || running;

  return (
    <ModalShell
      title="Add watermark"
      className="watermark-dialog"
      busy={running}
      field={readOnly ? close : first}
      onCancel={onCancel}
    >
      {(heading) => (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!disabled) onAdd();
          }}
        >
          {heading}
          {readOnly && (
            <p className="watermark-dialog__refusal" role="alert">
              {refusal}
            </p>
          )}
          <fieldset className="export-dialog__group export-dialog__group--row">
            <legend className="export-dialog__legend">Watermark</legend>
            {KINDS.map(({ value, label }) => (
              <label key={value} className="export-dialog__choice">
                <input
                  ref={value === draft.kind ? first : undefined}
                  type="radio"
                  name={kindName}
                  value={value}
                  checked={draft.kind === value}
                  disabled={disabled}
                  onChange={() => onSwitch(value)}
                />
                {label}
              </label>
            ))}
          </fieldset>
          {draft.kind === 'text' ? (
            <>
              <label className="watermark-dialog__field">
                Watermark text
                <input
                  type="text"
                  value={draft.text}
                  disabled={disabled}
                  spellCheck={false}
                  onChange={(event) => onEdit({ ...draft, text: event.target.value })}
                />
              </label>
              <Slider
                label={`Size: ${draft.fontSize} pt`}
                name="Size"
                range={FONT_SIZE_RANGE}
                value={draft.fontSize}
                disabled={disabled}
                onChange={(fontSize) => onEdit({ ...draft, fontSize })}
              />
            </>
          ) : (
            <>
              <div className="watermark-dialog__picture">
                <FilePicker
                  label={draft.picture === null ? 'Choose picture…' : 'Choose another picture…'}
                  accept="image/png,image/jpeg,.png,.jpg,.jpeg"
                  disabled={disabled}
                  onPick={onPickPicture}
                />
                <span className="watermark-dialog__summary">{pictureSummary(draft.picture)}</span>
              </div>
              <Slider
                label={`Size: ${draft.scalePercent}% of the page`}
                name="Size"
                range={SCALE_PERCENT_RANGE}
                value={draft.scalePercent}
                disabled={disabled}
                onChange={(scalePercent) => onEdit({ ...draft, scalePercent })}
              />
            </>
          )}
          <Slider
            label={`Opacity: ${draft.opacityPercent}%`}
            name="Opacity"
            range={OPACITY_PERCENT_RANGE}
            value={draft.opacityPercent}
            disabled={disabled}
            onChange={(opacityPercent) => onEdit({ ...draft, opacityPercent })}
          />
          <Slider
            label={`Angle: ${draft.rotationDegrees}°`}
            name="Angle"
            range={ROTATION_RANGE}
            value={draft.rotationDegrees}
            disabled={disabled}
            onChange={(rotationDegrees) => onEdit({ ...draft, rotationDegrees })}
          />
          <Choices legend="Placement">
            {[
              { behind: false, label: 'Over the page content' },
              { behind: true, label: 'Behind the page content' },
            ].map(({ behind, label }) => (
              <label key={label} className="export-dialog__choice">
                <input
                  type="radio"
                  name={layerName}
                  checked={draft.behind === behind}
                  disabled={disabled}
                  onChange={() => onEdit({ ...draft, behind })}
                />
                {label}
              </label>
            ))}
          </Choices>
          <Choices legend="Pages">
            {PAGES.map(({ value, label }) => (
              <label key={value} className="export-dialog__choice">
                <input
                  type="radio"
                  name={pagesName}
                  value={value}
                  checked={draft.pages === value}
                  disabled={disabled}
                  onChange={() => onEdit({ ...draft, pages: value })}
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
              onChange={(event) => onEdit({ ...draft, pages: 'custom', customRange: event.target.value })}
            />
          </Choices>
          {error !== null && (
            <p id={errorId} className="watermark-dialog__error" role="alert">
              {error}
            </p>
          )}
          <div className="dialog__actions">
            <button ref={close} type="button" onClick={onCancel} disabled={running}>
              {readOnly ? 'Close' : 'Cancel'}
            </button>
            {!readOnly && (
              <button
                type="submit"
                className="button--primary"
                aria-disabled={running}
                aria-describedby={error !== null ? errorId : undefined}
              >
                {running ? 'Adding…' : 'Add'}
              </button>
            )}
          </div>
        </form>
      )}
    </ModalShell>
  );
}

function Choices({ legend, children }: { readonly legend: string; readonly children: ReactNode }) {
  return (
    <fieldset className="export-dialog__group">
      <legend className="export-dialog__legend">{legend}</legend>
      {children}
    </fieldset>
  );
}

interface SliderProps {
  /** What the slider shows, its value included. */
  readonly label: string;
  /** Its accessible name, without the value (which the slider reports itself). */
  readonly name: string;
  readonly range: SliderRange;
  readonly value: number;
  readonly disabled: boolean;
  readonly onChange: (value: number) => void;
}

function Slider({ label, name, range, value, disabled, onChange }: SliderProps) {
  const id = useId();
  return (
    <div className="watermark-dialog__slider">
      <label htmlFor={id}>{label}</label>
      <input
        id={id}
        type="range"
        aria-label={name}
        aria-valuetext={label}
        min={range.min}
        max={range.max}
        step={1}
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </div>
  );
}
