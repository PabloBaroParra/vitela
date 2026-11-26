// The viewer's edits: markup over a text selection, an annotation drawn with
// a tool (ink, shape, note, free text, image stamp), a change to one of the
// session's annotations, a step of the caller's own (a form fill), and undo/redo of the document's history (any step: an annotation here, or a page edit made in
// Organize). Sends them to the engine one at a time and reports every
// edited document. Container logic only.
//
// The rules are the core's: whether the document lets annotations be edited
// is known at open (`annotationEditingAllowed`) and is asked again on every
// edit (`EditNotPermitted`), as Organize does with page assembly. The engine
// refreshes its preview only when a step changes what pdfium draws, inside
// the same request.
//
// Steps run in the order they were asked for: an undo pressed while a
// three-line markup is still being added waits for its last line. A markup
// or a drawn annotation is refused while any step is still running (an undo
// may be putting another page where it was made), and is checked once more
// right before it goes in: if the page at its position is no longer the one
// it was made on (`pageKey`), nothing is added. A change to an annotation
// waits its turn instead (an arrow key repeats faster than the core
// answers) and is built from the annotation as it is by then (`change`).
// The report (`onEdited`) is made even if the viewer went away meanwhile: the core has
// the edit either way.

import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import { EDIT_NOT_PERMITTED, errorKind, errorMessage } from '@vitela/core';
import type {
  Annotation,
  AnnotationEdit,
  DocumentInfo,
  Edit,
  EditedDocument,
  Engine,
  FormField,
  MarkupKind,
  PageRects,
  Rgb,
} from '@vitela/core';
import type { OpenDocument } from '../document/useDocument';
import type { EditBuild } from './editBuilds';
import { sameGeometry } from './editGeometry';
import { markupEdits } from './geometry';

/** The markup colour of the native shells (their `DEFAULT_COLOR`). */
export const MARKUP_COLOR: Rgb = { r: 255, g: 220, b: 0 };

type PageLayout = Pick<DocumentInfo, 'pages' | 'pageIds'>;

/**
 * What sits at position `index`: the page and how it is turned. Two equal
 * keys mean a selection made on one still fits the other.
 */
export function pageKey(info: PageLayout, index: number): string {
  return `${info.pageIds[index] ?? index}/${info.pages[index]?.rotationDegrees ?? 0}`;
}

const PAGE_CHANGED = 'The page changed before it could be marked up. Select the text again.';
const PAGE_CHANGED_UNDER_TOOL = 'The page changed before the annotation could be added. Nothing was added.';
const GONE = 'The annotation is no longer there. Nothing was changed.';
const PAGE_CHANGED_UNDER_EDIT = 'The page changed before the annotation could be. Nothing was changed.';
const MOVED_UNDER_DRAG = 'The annotation changed while it was being dragged. Try again.';

/**
 * How a markup ended: every line went in (`marked`), some did before one
 * failed (`partial`), none did (`failed`), or nothing was asked of the core
 * (`refused`: not allowed, a step still running, no lines). `partial` and
 * `failed` leave a `failure` to show; `refused` does not.
 */
export type MarkupOutcome = 'marked' | 'partial' | 'failed' | 'refused';

/**
 * How one drawn annotation ended: added, refused before reaching the core
 * (see `MarkupOutcome`), or failed with the core's error. A `failed` whose
 * kind the caller said it shows itself leaves no `failure`.
 */
export type AddOutcome =
  | { readonly status: 'added' }
  | { readonly status: 'refused' }
  | { readonly status: 'failed'; readonly kind: string | null; readonly message: string };

/** How a change to an annotation ended; `changed` also when there was nothing to change. */
export type ChangeOutcome = { readonly status: 'changed' } | Exclude<AddOutcome, { status: 'added' }>;

export interface ChangeOptions {
  /** The annotation as it was when a drag began: the change is dropped if it has moved since. */
  readonly grabbed?: Annotation;
  /** Kinds of error the caller shows itself (see `AddOutcome`). */
  readonly shownByCaller?: readonly string[];
}

/** An edit to add, or what builds it inside its step (see `add`). */
export type PendingEdit = AnnotationEdit | (() => Promise<AnnotationEdit>);

/** How a run of queued steps ended: how many went in, whether all did, and the error that stopped it. */
interface QueueResult {
  readonly applied: number;
  readonly complete: boolean;
  readonly error?: unknown;
}

/** A step that must not run (what it was meant for is not as it was), and what to say about it. */
class StepDropped extends Error {}

/** How a step that did not go in is told to its caller. */
function failed(error: unknown) {
  const kind = error instanceof StepDropped ? null : (errorKind(error) ?? null);
  return { status: 'failed', kind, message: errorMessage(error) } as const;
}

/** The session as the core last described it. */
export interface Session {
  readonly info: PageLayout;
  readonly annotations: readonly Annotation[];
  /** Unset until the first step answers, unless the document already had them. */
  readonly formFields?: readonly FormField[];
}

export function useAnnotationEdits(engine: Engine, document: OpenDocument, onEdited: (edited: EditedDocument) => void) {
  const [running, setRunning] = useState(0);
  const [failure, setFailure] = useState<string | null>(null);
  const [denied, setDenied] = useState(false);
  /** Settles once every step asked for so far has ended. */
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  /** Synchronous twin of `running`: two clicks in one frame must not both get in. */
  const runningNow = useRef(0);
  /**
   * The pages and annotations as the core last described them: from each step's result as
   * soon as it lands, ahead of the re-render that brings them as a prop.
   */
  const latest = useRef<Session>({ info: document.info, annotations: document.annotations ?? [], formFields: document.formFields });
  useLayoutEffect(() => {
    latest.current = { info: document.info, annotations: document.annotations ?? [], formFields: document.formFields };
  }, [document.info, document.annotations, document.formFields]);
  /** A change still waiting when the viewer goes away is not made behind the next screen. */
  const mounted = useRef(true);
  useLayoutEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const { handle } = document;
  const allowed = document.info.annotationEditingAllowed && !denied;

  /**
   * Runs `steps` after every step already asked for; stops at the first
   * refusal. A step that answers `null` had nothing to do.
   */
  const enqueue = useCallback(
    (steps: readonly (() => Promise<EditedDocument | null>)[], shownByCaller: readonly string[] = []): Promise<QueueResult> => {
      runningNow.current += 1;
      setRunning((count) => count + 1);
      const run = queue.current.then(async (): Promise<QueueResult> => {
        setFailure(null);
        let applied = 0;
        try {
          for (const step of steps) {
            const edited = await step();
            applied += 1;
            if (!edited) continue;
            latest.current = edited;
            onEdited(edited);
          }
          return { applied, complete: true };
        } catch (error) {
          const kind = errorKind(error);
          if (kind === EDIT_NOT_PERMITTED) setDenied(true);
          if (!kind || !shownByCaller.includes(kind)) setFailure(errorMessage(error));
          return { applied, complete: false, error };
        } finally {
          runningNow.current -= 1;
          setRunning((count) => count - 1);
        }
      });
      queue.current = run;
      return run;
    },
    [onEdited],
  );

  /**
   * One `addMarkup` per line of a selection on page `pageIndex`, which held
   * the page `expectedKey` (see `pageKey`) when it was selected: one undo
   * step each. See `MarkupOutcome` for how it ends.
   */
  const markup = useCallback(
    async (kind: MarkupKind, pageIndex: number, expectedKey: string, rects: PageRects): Promise<MarkupOutcome> => {
      if (!allowed || runningNow.current > 0) return 'refused';
      const edits = markupEdits(pageIndex, rects, kind, MARKUP_COLOR);
      if (edits.length === 0) return 'refused';
      const { applied, complete } = await enqueue(
        edits.map((edit, line) => () => {
          if (line === 0 && pageKey(latest.current.info, pageIndex) !== expectedKey) {
            return Promise.reject(new StepDropped(PAGE_CHANGED));
          }
          return engine.applyEdit(handle, edit);
        }),
      );
      if (complete) return 'marked';
      return applied > 0 ? 'partial' : 'failed';
    },
    [allowed, enqueue, engine, handle],
  );

  /**
   * One drawn annotation on page `pageIndex`, which held the page
   * `expectedKey` when the gesture began: one undo step. `edit` may be a
   * function that builds it (asking the core, say, where a stamp goes): it
   * runs inside the step, after the page check, so the queue is busy from
   * the call on and a second gesture meanwhile is refused before it asks the
   * core anything; a build that throws fails the step. Errors of a kind in
   * `shownByCaller` (say `EncodingGap`, for a dialog that stays open with
   * it) are returned without becoming the viewer's `failure`. `onAdded`
   * gets the new annotation's id in the same step, before the edited
   * document is reported: what the caller keeps for it (a stamp's image) is
   * there by the time the layer draws it.
   */
  const add = useCallback(
    async (
      edit: PendingEdit,
      pageIndex: number,
      expectedKey: string,
      shownByCaller: readonly string[] = [],
      onAdded?: (id: number) => void,
    ): Promise<AddOutcome> => {
      if (!allowed || runningNow.current > 0) return { status: 'refused' };
      const { complete, error } = await enqueue(
        [
          async () => {
            if (pageKey(latest.current.info, pageIndex) !== expectedKey) throw new StepDropped(PAGE_CHANGED_UNDER_TOOL);
            const built = typeof edit === 'function' ? await edit() : edit;
            const edited = await engine.applyEdit(handle, built);
            if (edited.addedAnnotationId !== null) onAdded?.(edited.addedAnnotationId);
            return edited;
          },
        ],
        shownByCaller,
      );
      return complete ? { status: 'added' } : failed(error);
    },
    [allowed, enqueue, engine, handle],
  );

  /**
   * One change to annotation `id`, whose page was `expectedKey` when it was
   * asked for: one undo step, after every step already asked for. `build`
   * gets the annotation as it is by then, so changes asked for in a row each
   * start where the last one ended; one that answers `null` records nothing.
   */
  const change = useCallback(
    async (
      id: number,
      expectedKey: string,
      build: EditBuild,
      { grabbed, shownByCaller }: ChangeOptions = {},
    ): Promise<ChangeOutcome> => {
      if (!allowed) return { status: 'refused' };
      const { complete, error } = await enqueue(
        [
          async () => {
            if (!mounted.current) return null;
            const { info, annotations } = latest.current;
            const current = annotations.find((annotation) => annotation.id === id);
            const page = current && info.pages[current.pageIndex];
            if (!page) throw new StepDropped(GONE);
            if (pageKey(info, current.pageIndex) !== expectedKey) throw new StepDropped(PAGE_CHANGED_UNDER_EDIT);
            if (grabbed && !sameGeometry(grabbed, current)) throw new StepDropped(MOVED_UNDER_DRAG);
            const edit = build(current, page);
            return edit && engine.applyEdit(handle, edit);
          },
        ],
        shownByCaller,
      );
      return complete ? { status: 'changed' } : failed(error);
    },
    [allowed, enqueue, engine, handle],
  );

  /**
   * One step of the caller's own, after every step already asked for:
   * `build` gets the session as the core last described it and answers the
   * edit to apply, or `null` to record nothing (a build that throws fails the
   * step). Not held to the annotating permission: the caller checks its own,
   * and the core asks again. See `change` for `shownByCaller`.
   */
  const apply = useCallback(
    async (build: (session: Session) => Edit | null, shownByCaller: readonly string[] = []): Promise<ChangeOutcome> => {
      const { complete, error } = await enqueue(
        [
          async () => {
            if (!mounted.current) return null;
            const edit = build(latest.current);
            return edit && engine.applyEdit(handle, edit);
          },
        ],
        shownByCaller,
      );
      return complete ? { status: 'changed' } : failed(error);
    },
    [enqueue, engine, handle],
  );

  /** Shows a caller's own notice as the failure, until the next step starts. */
  const notify = useCallback((message: string) => setFailure(message), []);

  const canUndo = !!document.canUndo;
  const canRedo = !!document.canRedo;
  const undo = useCallback(async () => {
    if (canUndo) await enqueue([() => engine.undo(handle)]);
  }, [canUndo, enqueue, engine, handle]);
  const redo = useCallback(async () => {
    if (canRedo) await enqueue([() => engine.redo(handle)]);
  }, [canRedo, enqueue, engine, handle]);

  /** Settles once every step asked for so far has ended (a save waits for it). */
  const settled = useCallback(() => queue.current, []);

  return { allowed, pending: running > 0, failure, canUndo, canRedo, markup, add, change, apply, notify, undo, redo, settled };
}
