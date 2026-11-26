// The Compress button, what the last compression did, and its dialog.
// Presentational: the state lives in `useCompress`, owned by the workspace.

import { useEffect, useRef } from 'react';
import { StatusLine } from '../../ui/StatusLine';
import { CompressDialog } from './CompressDialog';
import { compressStatus } from './compressText';
import type { CompressActivity, CompressSession } from './useCompress';

interface CompressControlsProps {
  readonly session: CompressSession;
  readonly documentName: string;
}

export function CompressControls({ session, documentName }: CompressControlsProps) {
  const { activity, open, select, cancel, compress } = session;
  const busy = activity.kind === 'checking' || activity.kind === 'choosing' || activity.kind === 'compressing';
  const button = useRef<HTMLButtonElement>(null);

  // The dialog gives the focus back when it goes away, whatever the answer.
  const wasChoosing = useRef(false);
  useEffect(() => {
    const choosing = activity.kind === 'choosing';
    if (wasChoosing.current && !choosing) button.current?.focus();
    wasChoosing.current = choosing;
  }, [activity.kind]);

  return (
    <>
      {/* Not `disabled`: a disabled button drops the keyboard focus to the page. */}
      <button
        ref={button}
        type="button"
        className="compress-button"
        title="Compress"
        onClick={busy ? undefined : () => void open()}
        aria-disabled={busy}
      >
        {/* A narrow editor shows only the icon; the word stays the button's name. */}
        <CompressIcon />
        <span className="compress-button__label">Compress</span>
      </button>
      <CompressStatus activity={activity} documentName={documentName} />
      {activity.kind === 'choosing' && (
        <CompressDialog
          presets={activity.presets}
          preset={activity.preset}
          refusal={activity.refusal}
          signaturesWillBreak={activity.signaturesWillBreak}
          onSelect={select}
          onCompress={() => void compress()}
          onCancel={cancel}
        />
      )}
    </>
  );
}

/** The home page's Compress icon: two arrows pressing on a line. */
function CompressIcon() {
  return (
    <svg
      className="compress-button__icon"
      viewBox="0 0 24 24"
      width="18"
      height="18"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3.5 12h17M12 3.5v5.6M8.7 5.8 12 9.1l3.3-3.3M12 20.5v-5.6M8.7 18.2 12 14.9l3.3 3.3" />
    </svg>
  );
}

function CompressStatus({ activity, documentName }: { readonly activity: CompressActivity; readonly documentName: string }) {
  switch (activity.kind) {
    case 'compressing':
      return <StatusLine>Compressing…</StatusLine>;
    case 'failed':
      return <StatusLine tone="error">{`Could not compress ${documentName}: ${activity.message}`}</StatusLine>;
    case 'done':
      return <StatusLine>{compressStatus(activity.result)}</StatusLine>;
    default:
      return null;
  }
}
