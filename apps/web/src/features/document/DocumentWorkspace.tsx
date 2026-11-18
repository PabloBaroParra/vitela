// Container: opening a PDF (picker, drop, password) plus the screen of the
// open document: the viewer, or Organize. What shows before a document is
// open is the caller's: it gets the open action (picker plus activity) to place.

import { useState, type ReactNode } from 'react';
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

/** The screens of an open document. */
export type DocumentScreen = 'view' | 'organize';

const SCREENS = [
  { value: 'view', label: 'View' },
  { value: 'organize', label: 'Organize' },
] as const;

interface DocumentWorkspaceProps {
  readonly engine: Engine;
  /** The page shown while no document is open, built around the open action. */
  readonly intro: (action: ReactNode) => ReactNode;
  /** The screen every document opens in: the tool's own (`/organize`), else the viewer. */
  readonly initialScreen?: DocumentScreen;
}

export function DocumentWorkspace({ engine, intro, initialScreen = 'view' }: DocumentWorkspaceProps) {
  const { state, open, submitPassword, cancelPassword, revise } = useDocument(engine);
  const { document, activity } = state;
  const [screen, setScreen] = useState<DocumentScreen>(initialScreen);
  const [screenFor, setScreenFor] = useState(document?.generation);
  // Every document opens in the initial screen (adjusting state while rendering).
  if (screenFor !== document?.generation) {
    setScreenFor(document?.generation);
    setScreen(initialScreen);
  }
  const busy = activity.kind === 'opening' || activity.kind === 'password';
  const dragging = useFileDrop(open, !busy);
  // Owned here, above the viewer, so editing screens inside it can flag edits.
  const save = useSave(engine, document);

  const picker = (
    <FilePicker
      label="Open PDF"
      accept="application/pdf,.pdf"
      onPick={open}
      pick={canPickWithHandle() ? pickPdfWithHandle : undefined}
      disabled={busy}
    />
  );

  return (
    <div className="workspace">
      {document ? (
        <div className="workspace__bar">
          {picker}
          <ActivityLine activity={activity} />
          {activity.kind === 'idle' && (
            <StatusLine>{`${document.name} — ${document.info.pageCount} ${document.info.pageCount === 1 ? 'page' : 'pages'}`}</StatusLine>
          )}
          <ViewSwitch views={SCREENS} current={screen} onChange={setScreen} />
          <SaveControls session={save} documentName={document.name} />
        </div>
      ) : (
        intro(
          <>
            {picker}
            <ActivityLine activity={activity} />
          </>,
        )
      )}
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
