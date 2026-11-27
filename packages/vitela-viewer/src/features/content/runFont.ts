// The font a run's camouflaged editor draws in (B26 T-259): the program the
// PDF embeds for it, loaded as a `FontFace`, when the browser can load it.
//
// Browsers load TrueType and OpenType programs; a bare CFF table or a Type 1
// program they do not, and a subset the font sanitizer rejects fails to load:
// then there is no family, and the editor falls back to the system family the
// font's name suggests (`fallbackFamilies`). A loaded face is kept for the
// page's base (its id, the core's base generation and the font's resource
// name), so a second editor over the same font loads nothing.

import type { DocumentHandle, Engine } from '@vitela/core';

const loaded = new Map<string, Promise<string | null>>();
let nextFace = 0;

/** Which run's font to load: where it is read, and what its face is kept under. */
export interface RunFontSource {
  readonly handle: DocumentHandle;
  readonly pageIndex: number;
  readonly runId: number;
  /** The page's id and the core's base generation: a face holds for that base only. */
  readonly pageId: number;
  readonly baseGeneration: number;
  readonly fontName: string;
}

/** The CSS family the run's embedded program loaded as, or `null` when there is none the browser takes. */
export function loadRunFont(engine: Engine, source: RunFontSource): Promise<string | null> {
  const key = `${source.handle}/${source.pageId}/${source.baseGeneration}/${source.fontName}`;
  let face = loaded.get(key);
  if (!face) {
    face = load(engine, source);
    loaded.set(key, face);
  }
  return face;
}

async function load(engine: Engine, { handle, pageIndex, runId }: RunFontSource): Promise<string | null> {
  if (typeof FontFace === 'undefined' || typeof document === 'undefined' || !document.fonts) return null;
  try {
    const program = await engine.textRunFontProgram(handle, pageIndex, runId);
    if (!program || (program.format !== 'trueType' && program.format !== 'openType')) return null;
    const family = `vitela-run-${nextFace++}`;
    const face = new FontFace(family, program.bytes as Uint8Array<ArrayBuffer>);
    await face.load();
    document.fonts.add(face);
    return family;
  } catch {
    // A program the browser will not take is drawn in the fallback family.
    return null;
  }
}
