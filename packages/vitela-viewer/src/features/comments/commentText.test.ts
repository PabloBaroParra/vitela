import { describe, expect, it } from 'vitest';
import { commentByline, formatPdfDate } from './commentText';

describe('formatPdfDate', () => {
  it('shows the day and time the file states, in its own time zone', () => {
    expect(formatPdfDate("D:20240305143059+02'00'")).toBe('2024-03-05 14:30');
    expect(formatPdfDate('D:20240305143059Z')).toBe('2024-03-05 14:30');
    expect(formatPdfDate('20240305')).toBe('2024-03-05');
  });

  it('shows only the precision there is', () => {
    expect(formatPdfDate('D:2024')).toBe('2024');
    expect(formatPdfDate('D:202403')).toBe('2024-03');
    expect(formatPdfDate('D:2024030514')).toBe('2024-03-05');
  });

  it('leaves anything else as the file has it', () => {
    for (const raw of ['yesterday', 'D:20241399', 'D:20240305996000', '5/3/2024', '']) expect(formatPdfDate(raw)).toBe(raw);
  });
});

describe('commentByline', () => {
  it('joins the author and date there are', () => {
    const line = (author: string | null, date: string | null) => commentByline({ author, date });
    expect(line('Ana', 'D:20240305')).toBe('Ana · 2024-03-05');
    expect(line('Ana', null)).toBe('Ana');
    expect(line(null, null)).toBe('');
    expect(line(' ', '')).toBe('');
  });
});
