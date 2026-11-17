// Container: opening a PDF (picker, drop, password) plus the screen of the
// open document: the viewer, or Organize.

import { useState } from 'react';
import type { Engine } from '@vitela/core';
import { FilePicker } from '../../ui/FilePicker';
import { canPickWithHandle, pickPdfWithHandle } from '../../platform/fileHandle';
import { MarkDirtyContext } from '../save/MarkDirtyContext';
import { SaveControls } from '../save/SaveControls';
import { useSave } from '../save/useSave';
import { StatusLine } from '../../ui/StatusLine';
import { ViewSwitch } from '../../ui/ViewSwitch';
import { OrganizeScreen } from '../organize/OrganizeScreen';
import { DocumentViewer } from '../viewer/DocumentViewer';
import { PasswordDialog } from './PasswordDialog';
import { useDocument, type Activity } from './useDocument';
import { useFileDrop } from './useFileDrop';

type Screen = 'view' | 'organize';

const SCREENS = [
  { value: 'view', label: 'View' },
  { value: 'organize', label: 'Organize' },
] as const;

interface DocumentWorkspaceProps {
  readonly engine: Engine;
}

export function DocumentWorkspace({ engine }: DocumentWorkspaceProps) {
  const { state, open, submitPassword, cancelPassword, revise } = useDocument(engine);
  const { document, activity } = state;
  const [screen, setScreen] = useState<Screen>('view');
  const [screenFor, setScreenFor] = useState(document?.generation);
  // Every document opens in the viewer (adjusting state while rendering).
  if (screenFor !== document?.generation) {
    setScreenFor(document?.generation);
    setScreen('view');
  }
  const busy = activity.kind === 'opening' || activity.kind === 'password';
  const dragging = useFileDrop(open, !busy);
  // Owned here, above the viewer, so editing screens inside it can flag edits.
  const save = useSave(engine, document);

  return (
    <div className="workspace">
      <div className="workspace__bar">
        <FilePicker
          label="Open PDF"
          accept="application/pdf,.pdf"
          onPick={open}
          pick={canPickWithHandle() ? pickPdfWithHandle : undefined}
          disabled={busy}
        />
        <ActivityLine activity={activity} />
        {activity.kind === 'idle' && document && (
          <StatusLine>{`${document.name} — ${document.info.pageCount} ${document.info.pageCount === 1 ? 'page' : 'pages'}`}</StatusLine>
        )}
        {document && <ViewSwitch views={SCREENS} current={screen} onChange={setScreen} />}
        {document && <SaveControls session={save} documentName={document.name} />}
        {activity.kind === 'idle' && !document && <StatusLine>Open a PDF or drop one anywhere on this window.</StatusLine>}
      </div>
      {document && (
        <MarkDirtyContext.Provider value={save.markDirty}>
          {/* Each screen mounts afresh on a switch: the viewer then lays out the pages as they are now. */}
          {screen === 'view' ? (
            <DocumentViewer key={document.generation} engine={engine} document={document} />
          ) : (
            <OrganizeScreen key={document.generation} engine={engine} document={document} onEdited={revise} />
          )}
        </MarkDirtyContext.Provider>
      )}
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
