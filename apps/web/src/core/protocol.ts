// The message protocol between the UI thread (core/client.ts) and the engine
// worker (worker/). Both sides import only these types, so neither depends on
// how the other is implemented.

import type { DocumentHandle, OpenedDocument, PagePoint, RenderedPage, SearchHit, TextSelection } from './types';

export type EngineRequest =
  | { readonly kind: 'open'; readonly bytes: Uint8Array; readonly password?: string }
  | {
      readonly kind: 'renderPage';
      readonly document: DocumentHandle;
      readonly pageIndex: number;
      readonly dpi: number;
    }
  | { readonly kind: 'close'; readonly document: DocumentHandle }
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
    };

export interface EngineResults {
  open: OpenedDocument;
  renderPage: RenderedPage;
  close: void;
  search: SearchHit[];
  selectText: TextSelection;
  selectWord: TextSelection;
}

/** UI -> worker. `id` correlates the reply. */
export interface RequestEnvelope {
  readonly id: number;
  readonly request: EngineRequest;
}

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
}
