// Container: opening a PDF (picker, drop, password) plus the screen of the
// open document: the viewer, or Organize. What shows before a document is
// open is the caller's: it gets the open action (picker plus activity) to place.

import { useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState, type ReactNode, type Ref } from 'react';
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
import { ExtractControls } from '../extract/ExtractControls';
import { SplitControls } from '../extract/SplitControls';
import { pageRunText } from '../extract/extractText';
import { extractBusy, useExtract } from '../extract/useExtract';
import { splitBusy, useSplit } from '../extract/useSplit';
import { PrintControls } from '../print/PrintControls';
import { printBusy, usePrint } from '../print/usePrint';
import { usePrintKey } from '../print/usePrintKey';
import { ProtectControls } from '../protect/ProtectControls';
import { protectBusy, useProtect } from '../protect/useProtect';
import { PropertiesControls } from '../properties/PropertiesControls';
import { propertiesBusy, useDocumentProperties } from '../properties/useDocumentProperties';
import { useContentGate } from '../content/useContentMode';
import { WatermarkControls } from '../watermark/WatermarkControls';
import { useWatermark, watermarkBusy } from '../watermark/useWatermark';
import { MenuButton } from '../../ui/MenuButton';
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
  const { state, open, submitPassword, cancelPassword, revise, nameSources, keepStampImage, pageCountNow } =
    useDocument(engine);
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
  // The document tools are one menu: every dialog it opens hands the focus back to its button.
  const toolsButton = useRef<HTMLButtonElement | null>(null);
  // Page images go where a compressed copy goes, after the same queued edits.
  const exportImages = useExportImages(engine, document, delivery ?? browserDelivery, save.settleEdits, readCurrentPage);
  // Print draws what a save would write, after the same queued edits, into a
  // frame inside the editor (never elsewhere on a host's page). Ctrl/⌘+P
  // prints it too while the key is the editor's.
  const frameHost = useCallback(() => rootRef.current, [rootRef]);
  const print = usePrint(engine, document, { settleEdits: save.settleEdits, frameHost });
  const startPrint = print.print;
  usePrintKey(document ? () => void startPrint() : null, scope);
  // A protected copy goes where a compressed one goes, after the same queued edits.
  const protect = useProtect(engine, document, delivery ?? browserDelivery, save.settleEdits);
  // An extracted PDF and a split's parts go there too, after the same queued edits.
  const extract = useExtract(engine, document, delivery ?? browserDelivery, save.settleEdits, pageCountNow);
  const split = useSplit(engine, document, delivery ?? browserDelivery, save.settleEdits, pageCountNow);
  // A properties change is an edit: it waits for the queued ones, joins the
  // history, marks the document dirty and is waited for by Save.
  const properties = useDocumentProperties(engine, document, {
    settleEdits: save.settleEdits,
    waitForEdits: save.waitForEdits,
    onEdited: revise,
    markDirty: save.markDirty,
  });
  // A watermark is an edit too, on the pages the reader picks.
  const watermark = useWatermark(engine, document, {
    settleEdits: save.settleEdits,
    waitForEdits: save.waitForEdits,
    onEdited: revise,
    markDirty: save.markDirty,
    currentPage: readCurrentPage,
    pageCountNow,
  });
  // "Edit content" is a mode of the viewer, turned on and off from Tools
  // (the viewer's toolbars have no room for its button). The gate is asked
  // once per document: a refused one keeps the item, off, with the core's
  // sentence. A new document, or Organize, turns the mode off.
  const contentGate = useContentGate(engine, document?.handle ?? null);
  const [contentFor, setContentFor] = useState<number | null>(null);
  const contentEditing = contentFor === document?.generation && screen === 'view' && contentGate.status === 'allowed';
  const toggleContentEditing = useCallback(() => {
    setScreen('view');
    setContentFor(contentEditing ? null : (document?.generation ?? null));
  }, [contentEditing, document?.generation]);
  const endContentEditing = useCallback(() => {
    setContentFor(null);
    toolsButton.current?.focus({ preventScroll: true });
  }, []);
  const changeScreen = useCallback((next: DocumentScreen) => {
    setScreen(next);
    if (next !== 'view') setContentFor(null);
  }, []);
  const pageToolsBusy = extractBusy(extract.activity) || splitBusy(split.activity);
  const openExtract = extract.open;
  const openSplit = split.open;
  const pageTools = useMemo(
    () => ({
      busy: pageToolsBusy,
      extract: (pages: { readonly from: number; readonly count: number }) => void openExtract(pageRunText(pages)),
      split: () => void openSplit(),
    }),
    [pageToolsBusy, openExtract, openSplit],
  );
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
            <ViewSwitch views={SCREENS} current={screen} onChange={changeScreen} />
            {/* Opened from the Tools menu below: here are their status lines and dialogs. */}
            <CompressControls session={compress} documentName={document.name} trigger={toolsButton} />
            <ExportImagesControls session={exportImages} trigger={toolsButton} />
            <PrintControls session={print} />
            <ProtectControls session={protect} documentName={document.name} trigger={toolsButton} />
            <WatermarkControls session={watermark} trigger={toolsButton} />
            <PropertiesControls session={properties} trigger={toolsButton} />
            {/* Extract and Split are opened from Organize: here are their status lines and dialogs. */}
            <ExtractControls session={extract} pageCount={document.info.pageCount} />
            <SplitControls session={split} pageCount={document.info.pageCount} />
            {/*
              The document tools as one menu button, at every width: the bar
              has no room for a button each (an embedding host's larger
              buttons already fill it), and one menu is one code path for the
              desktop and the phone.
            */}
            <MenuButton
              label="Tools"
              buttonRef={toolsButton}
              className="workspace__tools"
              icon={<ToolsIcon />}
              tools={[
                { label: 'Compress', disabled: compressBusy(compress.activity), onSelect: () => void compress.open() },
                { label: 'Export images', disabled: exportBusy(exportImages.activity), onSelect: () => void exportImages.open() },
                { label: 'Print', disabled: printBusy(print.activity), onSelect: () => void print.print() },
                { label: 'Protect', disabled: protectBusy(protect.activity), onSelect: () => void protect.open() },
                { label: 'Watermark', disabled: watermarkBusy(watermark.activity), onSelect: () => void watermark.open() },
                {
                  label: 'Document properties',
                  disabled: propertiesBusy(properties.activity),
                  onSelect: () => void properties.open(),
                },
                {
                  label: 'Edit content',
                  checked: contentEditing,
                  disabled: contentGate.status !== 'allowed',
                  description: contentGate.status === 'refused' ? contentGate.reason : undefined,
                  onSelect: toggleContentEditing,
                },
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
                    contentEditing={contentEditing}
                    onContentEditingEnd={endContentEditing}
                  />
                </ReportCurrentPageContext.Provider>
              ) : (
                <OrganizeScreen
                  key={document.generation}
                  engine={engine}
                  document={document}
                  onEdited={revise}
                  onSourcesNamed={nameSources}
                  pageTools={pageTools}
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

/**
 * "Tools" and a chevron, or (on a phone, CSS) three dots in a row: the bar
 * has no width there for the word.
 */
function ToolsIcon() {
  return (
    <>
      <span className="workspace__tools-label">Tools</span>
      <svg
        className="workspace__tools-chevron"
        viewBox="0 0 24 24"
        width="14"
        height="14"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.4"
        aria-hidden="true"
      >
        <path d="M6 9l6 6 6-6" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
      <svg className="workspace__tools-dots" viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
        <circle cx="5" cy="12" r="1.9" />
        <circle cx="12" cy="12" r="1.9" />
        <circle cx="19" cy="12" r="1.9" />
      </svg>
    </>
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
