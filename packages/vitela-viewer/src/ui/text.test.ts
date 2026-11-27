import { describe, expect, it } from 'vitest';
import { sentence } from './text';

describe('sentence', () => {
  it("turns the core's clause into a sentence", () => {
    expect(sentence('this cannot be compressed')).toBe('This cannot be compressed.');
    expect(sentence('Already one.')).toBe('Already one.');
    expect(sentence('  padded  ')).toBe('Padded.');
  });
});
