// Asks what to do about saving a signed document: cancel, save a copy (the
// opened file stays as it is), or save over it anyway. Presentational: it
// reports the answer. A native modal <dialog> keeps focus inside it and makes the
// rest of the page inert; Escape fires `cancel`. The wording follows the
// other shells' `write-signature-*` strings, shortened to one statement and
// one hint.

import { useEffect, useId, useRef } from 'react';

interface SignatureDialogProps {
  /** Save over the opened file, breaking the signature. */
  readonly onSaveAnyway: () => void;
  /** Save to a new file; the opened one, and its signature, are untouched. */
  readonly onSaveCopy: () => void;
  readonly onCancel: () => void;
}

export function SignatureDialog({ onSaveAnyway, onSaveCopy, onCancel }: SignatureDialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const bodyId = useId();

  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (!element.open) element.showModal();
    return () => element.close();
  }, []);

  return (
    <dialog
      ref={dialog}
      className="dialog dialog--warning"
      aria-labelledby={titleId}
      aria-describedby={bodyId}
      onCancel={(event) => {
        // The parent unmounts the dialog; do not let the browser close it first.
        event.preventDefault();
        onCancel();
      }}
    >
      <div className="dialog__icon" aria-hidden="true">
        <SignatureIcon />
      </div>
      <h2 id={titleId} className="dialog__title">
        {"Saving will break this document's signature"}
      </h2>
      <div id={bodyId} className="dialog__body">
        <p>
          Saving rewrites the file, so the signature no longer matches it. The signature is not removed: PDF readers
          will report it as invalid.
        </p>
        <p className="dialog__hint">Save a copy to leave the original file, and its valid signature, untouched.</p>
      </div>
      <div className="dialog__actions">
        {/* Cancel takes the focus: the safe answer is the default one. */}
        <button type="button" autoFocus onClick={onCancel}>
          Cancel
        </button>
        <button type="button" onClick={onSaveCopy}>
          Save a copy
        </button>
        <button type="button" className="button--primary" onClick={onSaveAnyway}>
          Save anyway
        </button>
      </div>
    </dialog>
  );
}

function SignatureIcon() {
  return (
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" strokeWidth="1.75">
      <path d="M12 3 4.5 6v5.5c0 4.5 3.2 8.2 7.5 9.5 4.3-1.3 7.5-5 7.5-9.5V6L12 3Z" strokeLinejoin="round" />
      <path d="M12 8.5v4.5" strokeLinecap="round" />
      <circle cx="12" cy="16" r="0.6" fill="currentColor" />
    </svg>
  );
}
