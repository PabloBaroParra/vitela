// The message protocol between the UI thread and the engine worker (worker/).
// createEngine.ts sends the one init message; core/client.ts sends requests
// and receives replies. Both sides import only these types, so neither
// depends on how the other is implemented.

import type {
  Annotation,
  Comment,
  CompressedDocument,
  CompressPreset,
  CompressRefusal,
  DocumentHandle,
  Edit,
  EditedDocument,
  FormField,
  FreeTextLayout,
  ImportedDocument,
  OpenedDocument,
  PagePoint,
  PageRect,
  PreparedImport,
  RenderedPage,
  SearchHit,
  TextSelection,
} from './types.js';

export type EngineRequest =
  | { readonly kind: 'open'; readonly bytes: Uint8Array; readonly password?: string }
  | {
      readonly kind: 'renderPage';
      readonly document: DocumentHandle;
      readonly pageIndex: number;
      readonly dpi: number;
    }
  | { readonly kind: 'close'; readonly document: DocumentHandle }
  | { readonly kind: 'save'; readonly document: DocumentHandle; readonly invalidateSignatures: boolean }
  | { readonly kind: 'applyEdit'; readonly document: DocumentHandle; readonly edit: Edit }
  | { readonly kind: 'undo'; readonly document: DocumentHandle }
  | { readonly kind: 'redo'; readonly document: DocumentHandle }
  | { readonly kind: 'prepareImport'; readonly bytes: Uint8Array; readonly password?: string }
  | { readonly kind: 'discardImport'; readonly id: number }
  | {
      readonly kind: 'importPrepared';
      readonly document: DocumentHandle;
      readonly ids: readonly number[];
      readonly index: number;
    }
  | { readonly kind: 'search'; readonly document: DocumentHandle; readonly query: string }
  | {
      readonly kind: 'selectText';
      readonly document: DocumentHandle;
      readonly pageIndex: number;
      readonly anchor: PagePoint;
      readonly focus: PagePoint;
    }
  | {
      readonly kind: 'selectWord';
      readonly document: DocumentHandle;
      readonly pageIndex: number;
      readonly point: PagePoint;
    }
  | { readonly kind: 'annotations'; readonly document: DocumentHandle }
  | { readonly kind: 'comments'; readonly document: DocumentHandle }
  | { readonly kind: 'formFields'; readonly document: DocumentHandle }
  | { readonly kind: 'freeTextLayout'; readonly contents: string; readonly widthPt: number; readonly heightPt: number }
  | {
      readonly kind: 'stampPlacement';
      readonly document: DocumentHandle;
      readonly pageIndex: number;
      readonly image: Uint8Array;
      readonly anchor: PagePoint;
    }
  | { readonly kind: 'compressPresets' }
  | { readonly kind: 'compressionRefusal'; readonly document: DocumentHandle }
  | { readonly kind: 'compressionInvalidatesSignatures'; readonly document: DocumentHandle }
  | {
      readonly kind: 'saveCompressed';
      readonly document: DocumentHandle;
      readonly preset: CompressPreset;
      readonly invalidateSignatures: boolean;
    };

export interface EngineResults {
  open: OpenedDocument;
  renderPage: RenderedPage;
  close: void;
  save: Uint8Array;
  applyEdit: EditedDocument;
  undo: EditedDocument;
  redo: EditedDocument;
  prepareImport: PreparedImport;
  discardImport: void;
  importPrepared: ImportedDocument;
  search: SearchHit[];
  selectText: TextSelection;
  selectWord: TextSelection;
  annotations: Annotation[];
  comments: Comment[];
  formFields: FormField[];
  freeTextLayout: FreeTextLayout;
  stampPlacement: PageRect;
  compressPresets: CompressPreset[];
  compressionRefusal: CompressRefusal | null;
  compressionInvalidatesSignatures: boolean;
  saveCompressed: CompressedDocument;
}

/** UI -> worker. `id` correlates the reply. */
export interface RequestEnvelope {
  readonly id: number;
  readonly request: EngineRequest;
}

/**
 * UI -> worker, once, before the engine loads: where to fetch the WASM from.
 * Without `assetBaseUrl` the worker uses the copies bundled next to it.
 * Requests sent before it wait until the engine has loaded.
 */
export interface InitMessage {
  readonly type: 'init';
  /**
   * An absolute directory URL serving pdf_wasm_bg.wasm, pdfium.js and
   * pdfium.wasm. Those files must be byte-identical to this package's.
   */
  readonly assetBaseUrl?: string;
  /** Per-file fetch timeout in milliseconds (default 60000). */
  readonly loadTimeoutMs?: number;
}

/** Everything the UI sends: the init message, then requests. */
export type ClientMessage = InitMessage | RequestEnvelope;

/** Worker -> UI. */
export type WorkerMessage =
  | { readonly type: 'ready' }
  | { readonly type: 'fatal'; readonly message: string }
  | { readonly type: 'result'; readonly id: number; readonly value: EngineResults[keyof EngineResults] }
  | {
      readonly type: 'error';
      readonly id: number;
      /** The failure's kind — the core's `Error.name`, e.g. `WrongPassword`. */
      readonly name: string;
      readonly message: string;
    };

/** The slice of `Worker` / `DedicatedWorkerGlobalScope` both ends talk through. */
export interface MessagePortLike<In, Out> {
  postMessage(message: Out, transfer?: Transferable[]): void;
  addEventListener(type: 'message', listener: (event: MessageEvent<In>) => void): void;
  /** Stops the other end for good (a `Worker` has it; the worker's own scope does not). */
  terminate?(): void;
}
