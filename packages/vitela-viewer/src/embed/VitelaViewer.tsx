// The embeddable editor: the workspace plus what a host page would otherwise
// have to wire itself. It starts (and stops) its own engine unless given one,
// opens what the host hands it (a File, bytes or a URL), sends saves to the
// host's `onSave` when there is one, and says when the engine is loading or
// failed to. Container: the markup is the workspace's.

import { useEffect, useImperativeHandle, useMemo, useRef, useState, type ReactNode, type Ref } from 'react';
import { createEngine, type Engine, type EngineOptions } from '@vitela/core';
import { useEngineStatus } from '../engine/useEngineStatus';
import { DocumentWorkspace, type DocumentScreen, type WorkspaceHandle } from '../features/document/DocumentWorkspace';
import type { EventScope } from '../features/document/editorScope';
import type { PdfSource } from '../features/document/useDocument';
import { callbackDelivery, type SaveCallback } from '../features/save/callbackDelivery';
import { StatusLine } from '../ui/StatusLine';
import { toPdfSource, type ViewerSource } from './source';

/** An engine the host started (`createEngine()`), shared or kept across mounts; `ready` is waited for when it has one. */
export type ViewerEngine = Engine & { readonly ready?: Promise<void> };

export interface VitelaViewerProps {
  /** The PDF to open once the engine is ready; a new value opens the new document. */
  readonly source?: ViewerSource;
  /** The document's name, for bytes and URLs (default: the URL's file name, else "document.pdf"). */
  readonly name?: string;
  /**
   * The engine to use; the host keeps it running after the editor unmounts.
   * Without one, the editor starts its own with `engineOptions` and stops it
   * when it unmounts.
   */
  readonly engine?: ViewerEngine;
  /** Options for the engine the editor starts itself (read once, at mount). Ignored when `engine` is given. */
  readonly engineOptions?: EngineOptions;
  /**
   * Receives every save: the document's bytes, its name and its media type
   * (`application/pdf`). Every other file the editor writes arrives here too
   * under its own name — a compressed copy (`application/pdf`), exported page
   * images (`image/png`, `image/jpeg`, one call per page). Without it, a save
   * writes over the opened file when the browser allows it, else downloads,
   * and every other file downloads.
   */
  readonly onSave?: SaveCallback;
  /** Whether the "Open PDF" picker is offered (default: yes). */
  readonly filePicker?: boolean;
  /** The screen every document opens in (default: the viewer). */
  readonly initialScreen?: DocumentScreen;
  /**
   * Which page events the editor takes (default `'editor'`): keys only with
   * the focus (or the last click or drop) inside it, dropped PDFs only over
   * it. `'window'` for a page that is all editor.
   */
  readonly eventScope?: EventScope;
  readonly ref?: Ref<VitelaViewerHandle>;
}

/** What a host can ask of a mounted editor. */
export interface VitelaViewerHandle {
  /** Opens `source` (once the engine is ready); the editor shows how it went. */
  open(source: ViewerSource, name?: string): void;
  /** Saves the open document as the Save button does; nothing when none is open. */
  save(): void;
}

/** A source waiting for the engine, numbered so each is opened exactly once. */
interface Request {
  readonly id: number;
  readonly source: PdfSource;
}

const NEVER_READY = new Promise<void>(() => {});
const READY = Promise.resolve();

export function VitelaViewer({
  source,
  name,
  engine,
  engineOptions,
  onSave,
  filePicker,
  initialScreen,
  eventScope = 'editor',
  ref,
}: VitelaViewerProps) {
  const own = useOwnEngine(engine === undefined, engineOptions);
  const active: ViewerEngine | null = engine ?? (own.kind === 'running' ? own.engine : null);
  const status = useEngineStatus(
    useMemo(() => ({ ready: active ? (active.ready ?? READY) : NEVER_READY }), [active]),
  );
  const failure = own.kind === 'failed' ? own.message : status.kind === 'failed' ? status.message : null;

  const workspace = useRef<WorkspaceHandle>(null);
  const [request, setRequest] = useState<Request | null>(null);
  const nextId = useRef(0);
  const ask = (pdf: PdfSource) => setRequest({ id: ++nextId.current, source: pdf });

  // The source prop is a request like any other: the latest one wins.
  useEffect(() => {
    if (source !== undefined) setRequest({ id: ++nextId.current, source: toPdfSource(source, name) });
  }, [source, name]);

  // Hand the latest request over once documents can be opened, exactly once.
  // Per engine: after a swap the new workspace starts empty, so the latest
  // request opens again there. `active` is a dependency for that reason.
  const opened = useRef<{ engine: ViewerEngine | null; id: number }>({ engine: null, id: 0 });
  useEffect(() => {
    if (!request || status.kind !== 'ready' || !workspace.current) return;
    if (opened.current.engine === active && opened.current.id === request.id) return;
    opened.current = { engine: active, id: request.id };
    workspace.current.open(request.source);
  }, [request, status.kind, active]);

  useImperativeHandle(
    ref,
    () => ({
      open: (next, nextName) => ask(toPdfSource(next, nextName)),
      save: () => workspace.current?.save(),
    }),
    [],
  );

  // The latest callback, behind a delivery that stays the same across renders.
  const saveTo = useRef(onSave);
  useEffect(() => {
    saveTo.current = onSave;
  }, [onSave]);
  const hasOnSave = onSave !== undefined;
  const delivery = useMemo(
    () => (hasOnSave ? callbackDelivery((bytes, savedName, type) => saveTo.current?.(bytes, savedName, type)) : undefined),
    [hasOnSave],
  );

  const standIn = standInFor(failure, status.kind === 'ready');
  if (!active) {
    // No engine yet (it starts after mount) or it could not start: the root, with the stand-in.
    return <div className="vitela workspace">{emptyState(standIn)}</div>;
  }
  return (
    <DocumentWorkspace
      // Document handles are numbered per engine: a new engine gets a new
      // workspace, and the old one closes its document in the old engine.
      key={engineKey(active)}
      ref={workspace}
      engine={active}
      intro={emptyState}
      initialScreen={initialScreen}
      standIn={standIn}
      eventScope={eventScope}
      filePicker={filePicker}
      delivery={delivery}
    />
  );
}

const engineKeys = new WeakMap<ViewerEngine, number>();
let nextEngineKey = 0;

/** A key that is the same for one engine and different for every other. */
function engineKey(engine: ViewerEngine): number {
  let key = engineKeys.get(engine);
  if (key === undefined) engineKeys.set(engine, (key = ++nextEngineKey));
  return key;
}

/** What shows while no document is open: the open action, or why there is none yet. */
function emptyState(action: ReactNode): ReactNode {
  return <div className="workspace__empty">{action}</div>;
}

function standInFor(failure: string | null, ready: boolean): ReactNode | undefined {
  if (failure !== null) return <StatusLine tone="error">{`The PDF engine failed to load: ${failure}`}</StatusLine>;
  return ready ? undefined : <StatusLine>Loading the PDF engine…</StatusLine>;
}

type OwnEngine =
  | { readonly kind: 'none' }
  | { readonly kind: 'running'; readonly engine: ReturnType<typeof createEngine> }
  | { readonly kind: 'failed'; readonly message: string };

/**
 * The engine the editor starts itself when the host gives none: started after
 * mount (never during a server render), terminated on unmount.
 */
function useOwnEngine(wanted: boolean, options: EngineOptions | undefined): OwnEngine {
  const [own, setOwn] = useState<OwnEngine>({ kind: 'none' });
  const optionsAtMount = useRef(options);
  useEffect(() => {
    if (!wanted) return;
    let started: ReturnType<typeof createEngine>;
    try {
      started = createEngine(optionsAtMount.current);
    } catch (error) {
      setOwn({ kind: 'failed', message: error instanceof Error ? error.message : String(error) });
      return;
    }
    setOwn({ kind: 'running', engine: started });
    return () => {
      started.terminate();
      setOwn({ kind: 'none' });
    };
  }, [wanted]);
  return wanted ? own : { kind: 'none' };
}
