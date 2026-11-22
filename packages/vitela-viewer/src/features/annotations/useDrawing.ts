// The drawing tools' state in the viewer: which tool is armed, the colour,
// and the note or text box waiting for its text. Turns a finished gesture
// into an edit through the viewer's queue (`useAnnotationEdits.add`): ink
// and shapes at once, notes and text boxes once their dialog is answered;
// a stamp's click goes to the stamps' `place`, which owns the picked image.
// Image picks a file through the stamps' `pick` and arms the stamp tool once
// it reads — unless the user chose a tool (or Escape) or picked again while
// it was loading: the latest choice wins. Container logic only.
//
// A tool stays armed until Select or Escape (several strokes make a
// drawing), and drops back to Select when annotating turns out to be
// forbidden. Every gesture keeps the page it began on (`pageKey`): the queue
// refuses it if another page is there by the time it goes in. A text box
// whose text the core refuses (`EncodingGap`) keeps its dialog open with the
// core's message; any other failure closes it and is the viewer's to show.
// A stroke or shape refused before reaching the core (a step started while it
// was drawn) is told through the viewer's notice (`notify`). A dialog whose
// text was sent cannot be cancelled until the core answers.

import { useCallback, useEffect, useRef, useState } from 'react';
import { ENCODING_GAP } from '@vitela/core';
import type { PageRect, Rgb } from '@vitela/core';
import { WHOLE_DOCUMENT, type EditorScope } from '../document/editorScope';
import { isTextField } from '../document/historyKeys';
import { drawnEdit, PALETTE, textEdit, type ArmedTool, type DrawTool, type Gesture, type Size } from './drawing';
import { freeTextFrame } from './geometry';
import type { AddOutcome, PendingEdit } from './useAnnotationEdits';
import type { PickStamp, PlaceStamp } from './useStamps';

/** The slice of `useAnnotationEdits` drawing needs. */
export interface DrawingEdits {
  readonly allowed: boolean;
  readonly add: (
    edit: PendingEdit,
    pageIndex: number,
    expectedKey: string,
    shownByCaller?: readonly string[],
    onAdded?: (id: number) => void,
  ) => Promise<AddOutcome>;
  /** Shows `message` where the viewer shows its edit failures. */
  readonly notify: (message: string) => void;
}

/** A note or text box placed on the page, waiting for its text. */
export interface Draft {
  readonly tool: 'note' | 'freeText';
  readonly pageIndex: number;
  readonly pageKey: string;
  readonly rect: PageRect;
  /** The box's own frame (sides swapped on a quarter-turned page): what the text lays out in. */
  readonly frame: Size;
  /** The text was sent and the core has not answered yet. */
  readonly busy: boolean;
  /** Why the core refused the text; the dialog stays open. */
  readonly error: string | null;
}

const NOT_NOW = 'Another change is still being made. Try again in a moment.';
/** A stroke or shape refused before reaching the core (a step started mid-gesture). */
export const NOT_DRAWN = 'Another change was still being made. Nothing was added.';

/** The slice of `useStamps` drawing needs. */
export interface DrawingStamps {
  readonly place: PlaceStamp;
  readonly pick: PickStamp;
}

export function useDrawing(
  edits: DrawingEdits,
  rotationOf: (pageIndex: number) => number,
  stamps: DrawingStamps,
  scope: EditorScope = WHOLE_DOCUMENT,
) {
  const [chosen, setChosen] = useState<DrawTool>('select');
  /** Counts the user's choices (a tool, Escape, a pick): a pick arms only if none came after it. */
  const choices = useRef(0);
  const [color, setColor] = useState<Rgb>(PALETTE[0]!.color);
  const [draft, setDraft] = useState<Draft | null>(null);
  const { allowed, add, notify } = edits;
  // Forbidden after all (the core said so): back to Select, the draft goes.
  const tool: DrawTool = allowed ? chosen : 'select';
  useEffect(() => {
    if (!allowed) setDraft(null);
  }, [allowed]);

  const choose = useCallback(
    (next: DrawTool) => {
      if (next !== 'select' && !allowed) return;
      choices.current += 1;
      setChosen(next);
    },
    [allowed],
  );

  // Escape with focus in the editor puts the pointer back to selecting text —
  // unless a text field or one of the editor's open dialogs has it (the
  // dialog's own Escape cancels it).
  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented) return;
      if (!scope.ownsEvent(event.target) || isTextField(event.target) || scope.modalOpen()) return;
      choices.current += 1;
      setChosen('select');
    };
    window.addEventListener('keydown', listener);
    return () => window.removeEventListener('keydown', listener);
  }, [scope]);

  const { place, pick } = stamps;
  /** Image: reads `file` and arms the stamp tool with it, if it is still the latest choice. */
  const pickImage = useCallback(
    (file: File) => {
      const choice = ++choices.current;
      void pick(file).then((picked) => {
        if (picked && choice === choices.current) setChosen('stamp');
      });
    },
    [pick],
  );

  const onGesture = useCallback(
    (pageIndex: number, pageKey: string, armed: ArmedTool, gesture: Gesture) => {
      if (armed === 'stamp') {
        if (gesture.kind === 'point') place(pageIndex, pageKey, gesture.point);
        return;
      }
      if (armed === 'ink' || armed === 'shape') {
        const edit = drawnEdit(armed, pageIndex, gesture, color);
        // A refusal leaves no failure of its own: say the drawing went nowhere.
        if (edit) {
          void add(edit, pageIndex, pageKey).then((outcome) => {
            if (outcome.status === 'refused') notify(NOT_DRAWN);
          });
        }
        return;
      }
      if (gesture.kind !== 'box') return;
      const { width, height } = freeTextFrame(gesture.rect, rotationOf(pageIndex));
      setDraft({ tool: armed, pageIndex, pageKey, rect: gesture.rect, frame: { width, height }, busy: false, error: null });
    },
    [add, color, notify, place, rotationOf],
  );

  const submit = useCallback(
    async (text: string) => {
      if (!draft || draft.busy) return;
      setDraft({ ...draft, busy: true, error: null });
      const outcome = await add(textEdit(draft.tool, draft.pageIndex, draft.rect, text), draft.pageIndex, draft.pageKey, [
        ENCODING_GAP,
      ]);
      // Another dialog may have replaced this one meanwhile: only touch this one.
      const settle = (next: Draft | null) => setDraft((shown) => (shown?.rect === draft.rect ? next : shown));
      if (outcome.status === 'added') settle(null);
      else if (outcome.status === 'refused') settle({ ...draft, busy: false, error: NOT_NOW });
      else if (outcome.kind === ENCODING_GAP) settle({ ...draft, busy: false, error: outcome.message });
      else settle(null);
    },
    [add, draft],
  );

  // Once the text is sent the edit is queued and will land: wait for it.
  const cancel = useCallback(() => setDraft((shown) => (shown?.busy ? shown : null)), []);
  /** The text changed: the core's last refusal no longer describes it. */
  const retyped = useCallback(() => setDraft((shown) => (shown?.error ? { ...shown, error: null } : shown)), []);

  return { tool, choose, color, setColor, draft, onGesture, pickImage, submit, cancel, retyped };
}
