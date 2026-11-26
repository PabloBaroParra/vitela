// Container: opening a PDF (picker, drop, password) plus the screen of the
// open document: the viewer, or Organize. What shows before a document is
// open is the caller's: it gets the open action (picker plus activity) to place.

import { useCallback, useEffect, useImperativeHandle, useRef, useState, type ReactNode, type Ref } from 'react';
import type { Engine } from '@vitela/core';
import { FilePicker } from '../../ui/FilePicker';
import { canPickWithHandle, pickPdfWithHandle } from '../../platform/fileHandle';
import { MarkDirtyContext, SaveWaitsForEditsContext } from '../save/MarkDirtyContext';
import { SaveControls } from '../save/SaveControls';
import { browserDelivery, type Delivery } from '../save/delivery';
import { CompressControls } from '../compress/CompressControls';
import { compressBusy, useCompress } from '../compress/useCompress';
import { ExportImagesControls } from '../exportImages/ExportImagesControls';
import { exportBusy, useExportImages } from '../exportImages/useExportImages';
import { ToolsMenu } from '../../ui/ToolsMenu';
import { useSave } from '../save/useSave';
import { StatusLine } from '../../ui/StatusLine';
import { ViewSwitch } from '../../ui/ViewSwitch';
import { OrganizeScreen } from '../organize/OrganizeScreen';
import { DocumentViewer } from '../viewer/DocumentViewer';
import { ReportCurrentPageContext } from '../viewer/CurrentPageContext';
import { PasswordDialog } from './PasswordDialog';
import { useDocument, type Activity, type PdfSource } from './useDocument';
import { EditorScopeContext, useEditorScope, type EventScope } from './editorScope';
import { useFileDrop } from './useFileDrop';

/** The screens of an open document. */
export type DocumentScreen = 'view' | 'organize';

const SCREENS = [
  { value: 'view', label: 'View' },
  { value: 'organize', label: 'Organize' },
] as const;

/** What a host can ask of the workspace, besides what its own controls offer. */
export interface WorkspaceHandle {
  /** Opens `source` as if the user had picked it (the editor shows how it went). */
  open(source: PdfSource): void;
  /** Saves the open document as the Save button does; nothing when none is open. */
  save(): void;
}

interface DocumentWorkspaceProps {
  readonly engine: Engine;
  /** The page shown while no document is open, built around the open action. */
  readonly intro: (action: ReactNode) => ReactNode;
  /** The screen every document opens in: the tool's own (`/organize`), else the viewer. */
  readonly initialScreen?: DocumentScreen;
  /**
   * Shown instead of the open action while documents cannot be opened yet
   * (the engine is still loading, or failed to). The workspace stays mounted
   * across that change, so the intro page does not remount and replay its
   * entrance animation.
   */
  readonly standIn?: ReactNode;
  /**
   * Which page events the editor takes. `'editor'` (the default, for a page
   * the editor shares with others): keys only with focus in the editor (or
   * after the user's last press or drop there), dropped PDFs only over it.
   * `'window'` (a page that is all editor): every key and drop on the window.
   * Either way only the editor's own dialogs hold its keys.
   */
  readonly eventScope?: EventScope;
  /** Whether the "Open PDF" picker is offered (default: yes). Dropping a PDF still opens it. */
  readonly filePicker?: boolean;
  /** Where a save goes. Default: over the opened file when the browser allows it, else a download. */
  readonly delivery?: Delivery;
  readonly ref?: Ref<WorkspaceHandle>;
}

export function DocumentWorkspace({
  engine,
  intro,
  initialScreen = 'view',
  standIn,
  eventScope,
  filePicker = true,
  delivery,
  ref,
}: DocumentWorkspaceProps) {
  const { state, open, submitPassword, cancelPassword, revise, nameSources, keepStampImage } = useDocument(engine);
  const { document, activity } = state;
  const [screen, setScreen] = useState<DocumentScreen>(initialScreen);
  const [screenFor, setScreenFor] = useState(document?.generation);
  // Every document opens in the initial screen (adjusting state while rendering).
  if (screenFor !== document?.generation) {
    setScreenFor(document?.generation);
    setScreen(initialScreen);
  }
  const busy = activity.kind === 'opening' || activity.kind === 'password';
  const { rootRef, scope } = useEditorScope(eventScope);
  const dragging = useFileDrop(open, !busy && standIn === undefined, scope);
  // Owned here, above the viewer, so editing screens inside it can flag edits.
  const save = useSave(engine, document, delivery);
  // A compressed copy goes where saves go (a download, or the host's onSave)
  // and waits for the same queued edits.
  const compress = useCompress(engine, document, delivery ?? browserDelivery, save.settleEdits);
  // The page the viewer last showed: Export images' "Current page", in either screen.
  const currentPage = useRef(0);
  const reportCurrentPage = useCallback((pageIndex: number) => void (currentPage.current = pageIndex), []);
  const readCurrentPage = useCallback(() => currentPage.current, []);
  // A new document starts on its first page, until its viewer says otherwise.
  useEffect(() => {
    currentPage.current = 0;
  }, [document?.generation]);
  // Page images go where a compressed copy goes, after the same queued edits.
  // On a phone the tool buttons are hidden: a dialog they opened hands the focus back to More tools.
  const moreButton = useRef<HTMLButtonElement | null>(null);
  const focusMore = useCallback(() => moreButton.current?.focus(), []);
  const exportImages = useExportImages(engine, document, delivery ?? browserDelivery, save.settleEdits, readCurrentPage);
  useImperativeHandle(ref, () => ({ open: (source) => open(source), save: save.save }), [open, save.save]);

  const picker = filePicker && (
    <FilePicker
      label="Open PDF"
      accept="application/pdf,.pdf"
      onPick={open}
      pick={canPickWithHandle() ? pickPdfWithHandle : undefined}
      disabled={busy}
    />
  );

  return (
    // `vitela` is the root class the editor's stylesheet is scoped to.
    <div ref={rootRef} className="vitela workspace">
      <EditorScopeContext.Provider value={scope}>
        {document ? (
          <div className="workspace__bar">
            {picker}
            <ActivityLine activity={activity} />
            {activity.kind === 'idle' && (
              <StatusLine>{`${document.name} — ${document.info.pageCount} ${document.info.pageCount === 1 ? 'page' : 'pages'}`}</StatusLine>
            )}
            <ViewSwitch views={SCREENS} current={screen} onChange={setScreen} />
            <CompressControls session={compress} documentName={document.name} focusFallback={focusMore} />
            <ExportImagesControls session={exportImages} focusFallback={focusMore} />
            {/*
              The same tools as one menu button: what a phone shows instead
              of the buttons above (CSS), at the same place in the DOM, so
              the tab order is the order on screen at every width.
            */}
            <ToolsMenu
              label="More tools"
              buttonRef={moreButton}
              className="workspace__more"
              icon={<MoreIcon />}
              tools={[
                { label: 'Compress', disabled: compressBusy(compress.activity), onSelect: () => void compress.open() },
                { label: 'Export images', disabled: exportBusy(exportImages.activity), onSelect: () => void exportImages.open() },
              ]}
            />
            <SaveControls session={save} documentName={document.name} />
          </div>
        ) : (
          intro(
            standIn ?? (
              <>
                {picker}
                <ActivityLine activity={activity} />
              </>
            ),
          )
        )}
        {document && (
          <MarkDirtyContext.Provider value={save.markDirty}>
            <SaveWaitsForEditsContext.Provider value={save.waitForEdits}>
              {/* Each screen mounts afresh on a switch: the viewer then lays out the pages as they are now. */}
              {screen === 'view' ? (
                <ReportCurrentPageContext.Provider value={reportCurrentPage}>
                  <DocumentViewer
                    key={document.generation}
                    engine={engine}
                    document={document}
                    onEdited={revise}
                    onStampImage={keepStampImage}
                  />
                </ReportCurrentPageContext.Provider>
              ) : (
                <OrganizeScreen
                  key={document.generation}
                  engine={engine}
                  document={document}
                  onEdited={revise}
                  onSourcesNamed={nameSources}
                />
              )}
            </SaveWaitsForEditsContext.Provider>
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
            {/* Organize takes the dropped files in (see OrganizeScreen). */}
            {document && screen === 'organize' ? 'Drop PDFs to add their pages' : 'Drop the PDF to open it'}
          </div>
        )}
      </EditorScopeContext.Provider>
    </div>
  );
}

/** Three dots in a row: more of the same kind. */
function MoreIcon() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
      <circle cx="5" cy="12" r="1.9" />
      <circle cx="12" cy="12" r="1.9" />
      <circle cx="19" cy="12" r="1.9" />
    </svg>
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
