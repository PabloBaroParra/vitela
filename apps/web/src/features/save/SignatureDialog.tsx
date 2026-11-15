// Asks whether to save a signed document anyway. Presentational: it reports
// the answer. A native modal <dialog> keeps focus inside it and makes the
// rest of the page inert; Escape fires `cancel`. The wording is the other
// shells' (the `write-signature-*` strings of the Linux locale).

import { useEffect, useId, useRef } from 'react';

interface SignatureDialogProps {
  readonly onProceed: () => void;
  readonly onCancel: () => void;
}

export function SignatureDialog({ onProceed, onCancel }: SignatureDialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();

  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (!element.open) element.showModal();
    return () => element.close();
  }, []);

  return (
    <dialog
      ref={dialog}
      className="dialog"
      aria-labelledby={titleId}
      onCancel={(event) => {
        // The parent unmounts the dialog; do not let the browser close it first.
        event.preventDefault();
        onCancel();
      }}
    >
      <h2 id={titleId} className="dialog__title">
        {"Saving will break this document's signature"}
      </h2>
      <p>This document is signed. Saving rewrites the file, so the signature will no longer match what it covers.</p>
      <p>
        It is not removed: the saved file still carries the signature, and PDF readers will report it as invalid rather
        than missing.
      </p>
      <p>To keep a copy that still verifies, cancel and save to a different file.</p>
      <div className="dialog__actions">
        {/* Cancel takes the focus: the safe answer is the default one. */}
        <button type="button" autoFocus onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className="button--primary" onClick={onProceed}>
          Save anyway
        </button>
      </div>
    </dialog>
  );
}
