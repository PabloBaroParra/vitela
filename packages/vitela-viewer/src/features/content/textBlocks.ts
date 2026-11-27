// The text-block edits of the "Edit content" mode (B26 T-261), pure: which
// paragraph a selected run belongs to, what a key does in the paragraph
// editor, what committing a paragraph or a new line asks for, and where a
// new line is expected once the page is read again.
//
// How a paragraph wraps, what a one-line block does with its other runs,
// which font a new line is painted in and how wide it measures are all the
// core's. This only decides what to ask it for.

import type { PagePoint, PageRect, TextBlock, TextBlockEdit } from '@vitela/core';

/** The size a new line is inserted at, in points: the GTK and Windows shells' default box. */
export const INSERT_TEXT_SIZE_PT = 14;

/** What the status row says while Insert text waits for a press on a page. */
export const INSERT_TEXT_PROMPT = 'Click the page where the text goes. Press Escape to cancel.';

/** About how wide a Helvetica glyph is for its size: the core measures the real width. */
const AVERAGE_ADVANCE = 0.5;

/** The paragraph run `runId` is part of — a block of more than one line — or `null`. */
export function paragraphOf(blocks: readonly TextBlock[], runId: number): TextBlock | null {
  return blocks.find((block) => block.lines.length > 1 && block.runIds.includes(runId)) ?? null;
}

/**
 * What a key does in the paragraph editor, as in the GTK shell's: Ctrl (or
 * Cmd) + Enter commits, Escape cancels, a plain Enter is a line break.
 */
export function paragraphKey(key: string, modifier: boolean): 'commit' | 'cancel' | 'type' {
  if (key === 'Escape') return 'cancel';
  if (key === 'Enter' && modifier) return 'commit';
  return 'type';
}

/**
 * What committing `draft` over `block` (on page position `page`) asks for:
 * a retype of the whole block by its runs — each line break one the reader
 * typed — or nothing for the same text. Text with nothing to show is the
 * core's to read as taking the block off the page.
 */
export function paragraphRetypeOf(page: number, block: TextBlock, draft: string): Extract<TextBlockEdit, { kind: 'retypeTextBlock' }> | null {
  const text = draft.replace(/\r\n?/g, '\n');
  if (text === block.text) return null;
  return { kind: 'retypeTextBlock', page, runIds: block.runIds, text };
}

/** What committing `draft` as a new line at `at` asks for; nothing when it shows nothing. */
export function insertionOf(page: number, at: PagePoint, draft: string): TextBlockEdit | null {
  if (draft.trim() === '') return null;
  return { kind: 'insertText', page, at, text: draft, sizePt: INSERT_TEXT_SIZE_PT };
}

/**
 * Where a line inserted at `at` is drawn, roughly (its width is the core's
 * to measure): `at` is its top-left corner in the page's own frame, so on a
 * page drawn turned the box turns about that corner with the page.
 */
export function insertedRect(at: PagePoint, text: string, sizePt: number, rotationDegrees: number): PageRect {
  const length = Math.max(1, text.length) * sizePt * AVERAGE_ADVANCE;
  switch (rotationDegrees) {
    case 90:
      return { left: at.x - sizePt, top: at.y, width: sizePt, height: length };
    case 180:
      return { left: at.x - length, top: at.y - sizePt, width: length, height: sizePt };
    case 270:
      return { left: at.x, top: at.y - length, width: sizePt, height: length };
    default:
      return { left: at.x, top: at.y, width: length, height: sizePt };
  }
}
