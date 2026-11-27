// The text-run edits of the "Edit content" mode (B26 T-259), pure: what a
// key or a drag does to the selected run, what a retype commits, which run is
// the edited one once the page is read again, and how the camouflaged editor
// over a run looks (its box on the page as drawn, its font).
//
// What an edit writes, whether it is allowed, how a second retype or move of
// the same run folds into its step, and whether the page holds a bold sibling
// are all the core's. This only decides what to ask it for.

import type { ContentRun, PageContent, PageRect, TextRunEdit, TextRunLook, TextRunStyle } from '@vitela/core';

/** An arrow key moves the selected run this far, in points. */
export const NUDGE_PT = 1;
/** With Shift held. */
export const NUDGE_SHIFT_PT = 10;
/** A press that travels this far (CSS px) before it is let go is a drag, not a click. */
export const DRAG_THRESHOLD_PX = 3;

/** The move an arrow key asks for, on the page as drawn (points, y down), or `null` for any other key. */
export function nudgeOf(key: string, shift: boolean): { dx: number; dy: number } | null {
  const step = shift ? NUDGE_SHIFT_PT : NUDGE_PT;
  switch (key) {
    case 'ArrowLeft':
      return { dx: -step, dy: 0 };
    case 'ArrowRight':
      return { dx: step, dy: 0 };
    case 'ArrowUp':
      return { dx: 0, dy: -step };
    case 'ArrowDown':
      return { dx: 0, dy: step };
    default:
      return null;
  }
}

/** Whether a press that has travelled `dx, dy` CSS px is a drag. */
export function isDrag(dx: number, dy: number): boolean {
  return Math.hypot(dx, dy) >= DRAG_THRESHOLD_PX;
}

/** What committing `text` over `run` asks for: a retype, a removal when cleared, nothing for the same text. */
export function retypeOf(run: ContentRun, text: string): TextRunEdit | null {
  if (text === run.text) return null;
  if (text === '') return { kind: 'removeTextRun', page: run.pageIndex, runId: run.id };
  return { kind: 'retypeTextRun', page: run.pageIndex, runId: run.id, text };
}

/** The four face toggles. */
export type FaceToggle = keyof TextRunLook;

/** `look` with `toggle` flipped. */
export function toggled(look: TextRunLook, toggle: FaceToggle): TextRunLook {
  return { ...look, [toggle]: !look[toggle] };
}

/**
 * The edited run in a page read after the edit: the run overlapping
 * `expected` (where it should be now) the most, one showing `text` first.
 * `null` when the run was removed (`text` is `null`) or nothing overlaps.
 * Ids are valid only against the read they came from, so the run is found
 * by place, not by id.
 */
export function followRun(content: PageContent, expected: PageRect, text: string | null): ContentRun | null {
  if (text === null) return null;
  let best: ContentRun | null = null;
  let bestScore = 0;
  for (const run of content.runs) {
    const overlap = overlapArea(run.rect, expected);
    if (overlap <= 0) continue;
    // Same text first, then the larger overlap.
    const score = (run.text === text ? 1e9 : 0) + overlap;
    if (score > bestScore) {
      best = run;
      bestScore = score;
    }
  }
  return best;
}

function overlapArea(a: PageRect, b: PageRect): number {
  const width = Math.min(a.left + a.width, b.left + b.width) - Math.max(a.left, b.left);
  const height = Math.min(a.top + a.height, b.top + b.height) - Math.max(a.top, b.top);
  // A box of no width (a run of spaces) still overlaps where it lies.
  return width >= 0 && height >= 0 ? Math.max(width, 0.01) * Math.max(height, 0.01) : 0;
}

/**
 * The CSS family list for a font with no program the browser can load: the
 * name's family part without its subset tag (`Arial` of `ABCDEF+Arial,Bold`,
 * `Times` of `Times-Roman`), the system face the standard fonts are usually
 * shown in, then the generic family it belongs to.
 */
export function fallbackFamilies(baseFont: string): string[] {
  const name = withoutSubsetTag(baseFont);
  const family = name.split(/[,-]/)[0]!.trim();
  const lower = name.toLowerCase();
  const families: string[] = family ? [family] : [];
  const generic = ['courier', 'mono', 'consol', 'typewriter'].some((word) => lower.includes(word))
    ? 'monospace'
    : ['times', 'serif', 'roman', 'georgia', 'garamond', 'book'].some((word) => lower.includes(word)) && !lower.includes('sans')
      ? 'serif'
      : 'sans-serif';
  const system = { courier: 'Courier New', times: 'Times New Roman', helvetica: 'Arial' }[family.toLowerCase()];
  if (system) families.push(system);
  families.push(generic);
  return families;
}

/** `ABCDEF+Name` without its six-letter subset tag. */
function withoutSubsetTag(baseFont: string): string {
  const plus = baseFont.indexOf('+');
  return plus === 6 && /^[A-Z]{6}$/.test(baseFont.slice(0, 6)) ? baseFont.slice(7) : baseFont;
}

/** Text render modes that stroke the outlines (PDF 32000-1 §9.3.6): the usual faked bold. */
const STROKING_RENDER_MODES = [1, 2, 5, 6];

/**
 * The weight and slant to draw a run in: its look when the core read one (a
 * queued restyle included), else what its font declares, its render mode and
 * its name say.
 */
export function faceOf(style: TextRunStyle | null, look: TextRunLook | null): { bold: boolean; italic: boolean } {
  if (look) return { bold: look.bold, italic: look.italic };
  if (!style) return { bold: false, italic: false };
  const lower = withoutSubsetTag(style.baseFont).toLowerCase();
  return {
    bold:
      style.bold ||
      STROKING_RENDER_MODES.includes(style.renderMode) ||
      ['bold', 'black', 'heavy', 'semibold', 'demi'].some((word) => lower.includes(word)),
    italic: style.italic || lower.includes('italic') || lower.includes('oblique'),
  };
}

/**
 * Where the editor over a run goes, in CSS px inside the page: laid along
 * the text, so on a page drawn turned it is as long as the drawn box is
 * tall, centred on that box and turned with the page.
 */
export function editorBox(
  rect: PageRect,
  rotationDegrees: number,
  scale: number,
): { left: number; top: number; width: number; height: number; rotation: number } {
  const quarter = rotationDegrees === 90 || rotationDegrees === 270;
  const width = (quarter ? rect.height : rect.width) * scale;
  const height = (quarter ? rect.width : rect.height) * scale;
  const centerX = (rect.left + rect.width / 2) * scale;
  const centerY = (rect.top + rect.height / 2) * scale;
  return { left: centerX - width / 2, top: centerY - height / 2, width, height, rotation: rotationDegrees };
}
