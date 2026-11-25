// How a comment's author and date read. The core hands the file's own date
// string over unparsed; what does not look like a PDF date is shown as it is.
// Pure.

import type { Comment } from '@vitela/core';

const PDF_DATE = /^(?:D:)?(\d{4})(\d{2})?(\d{2})?(\d{2})?(\d{2})?(\d{2})?(?:[Zz+-][\d']*)?$/;

/** A PDF date as `2024-03-05 14:30`, in the time zone the file wrote it in and with the precision it has. */
export function formatPdfDate(raw: string): string {
  const match = PDF_DATE.exec(raw.trim());
  if (!match) return raw;
  const [, year, month, day, hour, minute] = match;
  if (Number(month ?? 1) > 12 || Number(day ?? 1) > 31 || Number(hour ?? 0) > 23 || Number(minute ?? 0) > 59) return raw;
  const date = [year, month, day].filter(Boolean).join('-');
  return minute === undefined ? date : `${date} ${hour}:${minute}`;
}

/** Who wrote it and when, as far as the file says; empty for a session note. */
export function commentByline({ author, date }: Pick<Comment, 'author' | 'date'>): string {
  return [author?.trim(), date && formatPdfDate(date).trim()].filter(Boolean).join(' · ');
}
