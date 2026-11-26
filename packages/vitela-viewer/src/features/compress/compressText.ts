// What Compress says, as pure functions. The wording is the other shells'
// (Android's strings, which are the Windows shell's CompressionWording, in
// turn the GTK shell's), so one compression reads the same everywhere.
//
// `beforeBytes` is "uncompressed", never "the original": it is a save of the
// session, not the file on disk once anything was edited. Nothing predicts a
// saving: the core cannot answer that without running.

import type { CompressPreset, CompressRefusal } from '@vitela/core';
import type { SaveOutcome } from '../save/outcome';

/** The core's "default offer": visibly unchanged on screen, materially smaller on disk. */
export const DEFAULT_PRESET: CompressPreset = 'balanced';

/** A compressed save without its bytes: what the status line reports. */
export interface CompressReport {
  readonly beforeBytes: number;
  readonly afterBytes: number;
  readonly savedBytes: number;
  readonly reduced: boolean;
  readonly refusals: readonly CompressRefusal[];
}

/** How a finished compression ended, for the status line. */
export type CompressResult =
  /** A smaller copy, handed to the delivery. */
  | { readonly kind: 'reduced'; readonly report: CompressReport; readonly outcome: SaveOutcome }
  /** Nothing smaller came out: nothing was written. */
  | { readonly kind: 'noGain'; readonly report: CompressReport };

const PRESETS: Record<CompressPreset, { readonly name: string; readonly detail: string }> = {
  lossless: { name: 'Lossless', detail: 'Repack the file only. Every page renders exactly as it does now.' },
  balanced: {
    name: 'Balanced',
    detail: 'Also bring images down to 150 DPI. Visibly unchanged on screen, materially smaller on disk.',
  },
  small: {
    name: 'Small',
    detail: 'Also bring images down to 96 DPI. For when the file has to fit through something and the pixels matter less than that.',
  },
};

export function presetText(preset: CompressPreset): { readonly name: string; readonly detail: string } {
  return PRESETS[preset];
}

/** Powers of ten, the way the other shells count. */
export function humanSize(bytes: number): string {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${Math.round(bytes / 1_000)} kB`;
  return bytes === 1 ? '1 byte' : `${bytes} bytes`;
}

/** Truncated rather than rounded, so the number shown is one the file actually reached: 49.7% reads 49%. */
export function percentSmaller(report: CompressReport): number {
  return report.beforeBytes > 0 ? Math.floor((report.savedBytes * 100) / report.beforeBytes) : 0;
}

/** The name the compressed copy is downloaded under: "report.pdf" -> "report-compressed.pdf". */
export function compressedName(name: string): string {
  const stem = name.trim().replace(/\.pdf$/i, '').trim();
  return `${stem || 'document'}-compressed.pdf`;
}

/** The core's lower-case clause as a sentence. */
export function sentence(message: string): string {
  const text = message.trim();
  const capital = text.charAt(0).toUpperCase() + text.slice(1);
  return /[.!?]$/.test(capital) ? capital : `${capital}.`;
}

/** The status line for a finished compression. */
export function compressStatus(result: CompressResult): string {
  const { report } = result;
  const text =
    result.kind === 'noGain'
      ? `Nothing to gain: this document compresses to the same ${humanSize(report.beforeBytes)} an uncompressed save writes, so no file was written.`
      : `${humanSize(report.beforeBytes)} uncompressed → ${humanSize(report.afterBytes)} compressed (${percentSmaller(report)}% smaller). ${delivered(result.outcome)}`;
  // Both known refusals are asked before the run, so this is normally empty;
  // but the core's list is open-ended, and one this UI did not ask about must
  // still reach the reader.
  return report.refusals.length === 0
    ? text
    : `${text} Not everything could be done: ${report.refusals.map((refusal) => refusal.message).join(', ')}.`;
}

function delivered(outcome: SaveOutcome): string {
  switch (outcome.how) {
    case 'delivered':
    case 'wrote':
      return `Saved ${outcome.name}.`;
    case 'cancelled':
      return 'No file was written.';
    default:
      return `Downloaded ${outcome.name}.`;
  }
}
