// Container: opening a PDF (picker, drop, password) plus the viewer of the
// open document.

import type { Engine } from '../../core/types';
import { FilePicker } from '../../ui/FilePicker';
import { StatusLine } from '../../ui/StatusLine';
import { DocumentViewer } from '../viewer/DocumentViewer';
import { PasswordDialog } from './PasswordDialog';
import { useDocument, type Activity } from './useDocument';
import { useFileDrop } from './useFileDrop';

interface DocumentWorkspaceProps {
  readonly engine: Engine;
}

export function DocumentWorkspace({ engine }: DocumentWorkspaceProps) {
  const { state, open, submitPassword, cancelPassword } = useDocument(engine);
  const { document, activity } = state;
  const busy = activity.kind === 'opening' || activity.kind === 'password';
  const dragging = useFileDrop(open, !busy);

  return (
    <div className="workspace">
      <div className="workspace__bar">
        <FilePicker label="Open PDF" accept="application/pdf,.pdf" onPick={open} disabled={busy} />
        <ActivityLine activity={activity} />
        {activity.kind === 'idle' && document && (
          <StatusLine>{`${document.name} — ${document.info.pageCount} ${document.info.pageCount === 1 ? 'page' : 'pages'}`}</StatusLine>
        )}
        {activity.kind === 'idle' && !document && <StatusLine>Open a PDF or drop one anywhere on this window.</StatusLine>}
      </div>
      {document && <DocumentViewer key={document.generation} engine={engine} document={document} />}
      {activity.kind === 'password' && (
        <PasswordDialog
          name={activity.name}
          wrong={activity.wrong}
          checking={activity.checking}
          onSubmit={submitPassword}
          onCancel={cancelPassword}
        />
      )}
      {dragging && (
        <div className="drop-hint" aria-hidden="true">
          Drop the PDF to open it
        </div>
      )}
    </div>
  );
}

function ActivityLine({ activity }: { readonly activity: Activity }) {
  switch (activity.kind) {
    case 'opening':
      return <StatusLine>{`Opening ${activity.name}…`}</StatusLine>;
    case 'failed':
      return <StatusLine tone="error">{`Could not open ${activity.name}: ${activity.message}`}</StatusLine>;
    default:
      return null;
  }
}
