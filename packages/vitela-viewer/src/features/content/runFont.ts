// The fonts a run's camouflaged editor draws in (B26 T-259): the program the
// PDF embeds for it, loaded as a `FontFace`, when the browser can load it.
//
// Browsers load TrueType and OpenType programs; a bare CFF table or a Type 1
// program they do not, and a subset the font sanitizer rejects fails to load:
// then there is no family, and the editor falls back to the system family the
// font's name suggests (`fallbackFamilies`).
//
// One registry per open document, owned by the viewer: it adds its faces to
// `document.fonts` (the host page's, when embedded) and takes every one of
// them off again on `dispose`. A face is kept by its program — the bytes the
// core decodes, compared in full — not by the run's resource name (`F1`
// names another font in each form XObject) nor by the core's base
// generation (a checkpoint rewrites the base, not the program): every run
// drawn with the same program shares one face, and a font a retype inserts
// is a new program, so a new face. A read that failed is not kept; a program
// the browser refused is.

import type { DocumentHandle, Engine, FontProgram } from '@vitela/core';

export interface RunFontRegistry {
  /** The CSS family run `runId`'s embedded program loaded as, or `null` when there is none the browser takes. */
  load(pageIndex: number, runId: number): Promise<string | null>;
  /** Takes every face it added off the page; what lands afterwards adds nothing. */
  dispose(): void;
}

/** One program the registry was asked for: its bytes, and the family it loaded as (`null`: refused). */
interface Kept {
  readonly bytes: Uint8Array;
  readonly family: Promise<string | null>;
}

let nextFace = 0;

export function createRunFontRegistry(engine: Engine, handle: DocumentHandle): RunFontRegistry {
  /** By a hash of the bytes; each bucket compared byte for byte. */
  const kept = new Map<number, Kept[]>();
  const faces = new Set<FontFace>();
  let disposed = false;

  const faceFor = (program: FontProgram): Promise<string | null> => {
    const hash = fnv1a(program.bytes);
    const bucket = kept.get(hash) ?? [];
    const found = bucket.find((entry) => sameBytes(entry.bytes, program.bytes));
    if (found) return found.family;
    const family = addFace(program.bytes);
    bucket.push({ bytes: program.bytes, family });
    kept.set(hash, bucket);
    return family;
  };

  const addFace = async (bytes: Uint8Array): Promise<string | null> => {
    const family = `vitela-run-${nextFace++}`;
    try {
      const face = new FontFace(family, bytes as Uint8Array<ArrayBuffer>);
      await face.load();
      if (disposed) return null;
      document.fonts.add(face);
      faces.add(face);
      return family;
    } catch {
      // A program the browser will not take is drawn in the fallback family.
      return null;
    }
  };

  return {
    async load(pageIndex, runId) {
      if (disposed || typeof FontFace === 'undefined' || typeof document === 'undefined' || !document.fonts) return null;
      let program: FontProgram | null;
      try {
        program = await engine.textRunFontProgram(handle, pageIndex, runId);
      } catch {
        // Not kept: the next editor over the run asks again.
        return null;
      }
      if (disposed || !program || (program.format !== 'trueType' && program.format !== 'openType')) return null;
      return faceFor(program);
    },
    dispose() {
      disposed = true;
      for (const face of faces) document.fonts.delete(face);
      faces.clear();
      kept.clear();
    },
  };
}

/** 32-bit FNV-1a: only a bucket key, every hit is compared in full. */
function fnv1a(bytes: Uint8Array): number {
  let hash = 0x811c9dc5;
  for (const byte of bytes) hash = Math.imul(hash ^ byte, 0x01000193);
  return hash >>> 0;
}

function sameBytes(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let index = 0; index < a.length; index += 1) if (a[index] !== b[index]) return false;
  return true;
}
