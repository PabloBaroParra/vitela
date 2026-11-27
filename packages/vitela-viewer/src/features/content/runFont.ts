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
// them off again on `dispose`.
//
// Two keys. The cheap one, asked first, is the run's page id, its font's
// resource name and the core's base generation: an editor opened again over
// a run of the same font reads no bytes from the worker. Only a miss reads
// the program, and the face is kept by the program itself — the bytes the
// core decodes, compared in full — so every run drawn with the same program
// shares one face whatever its page, resource name (`F1` names another font
// in each form XObject) or base (a checkpoint rewrites the base, not the
// program), and a font a retype inserts (a new resource) is a new program,
// so a new face. A read that failed is kept under neither key; a program the
// browser refused, and a font with no program, are.
//
// Known gap of the cheap key: two form XObjects on one page that both name
// a different font `F1` share it, so the second's editor may draw in the
// first one's face (the fallback family is no better). The text is the
// core's either way; only the camouflage is off.

import type { DocumentHandle, Engine, FontProgram } from '@vitela/core';

/** The run whose font an editor wants, and the cheap key it is kept by. */
export interface RunFontAt {
  readonly pageIndex: number;
  readonly runId: number;
  readonly pageId: number;
  /** The font's key in the page's resources. */
  readonly fontName: string;
  readonly baseGeneration: number;
}

export interface RunFontRegistry {
  /** The CSS family the run's embedded program loaded as, or `null` when there is none the browser takes. */
  load(at: RunFontAt): Promise<string | null>;
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
  /** By page id, resource name and base generation: what was answered for them. */
  const byKey = new Map<string, Promise<string | null>>();
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
    load({ pageIndex, runId, pageId, fontName, baseGeneration }) {
      if (disposed || typeof FontFace === 'undefined' || typeof document === 'undefined' || !document.fonts) return Promise.resolve(null);
      const key = JSON.stringify([pageId, fontName, baseGeneration]);
      const known = byKey.get(key);
      if (known) return known;
      const family = engine.textRunFontProgram(handle, pageIndex, runId).then(
        (program) => {
          if (disposed || !program || (program.format !== 'trueType' && program.format !== 'openType')) return null;
          return faceFor(program);
        },
        () => {
          // Not kept: the next editor over the run asks again.
          if (byKey.get(key) === family) byKey.delete(key);
          return null;
        },
      );
      byKey.set(key, family);
      return family;
    },
    dispose() {
      disposed = true;
      for (const face of faces) document.fonts.delete(face);
      faces.clear();
      kept.clear();
      byKey.clear();
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
