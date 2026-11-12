// The message protocol between the UI thread (core/client.ts) and the engine
// worker (worker/). Both sides import only these types, so neither depends on
// how the other is implemented.

import type { DocumentInfo, RenderedPage } from './types';

export type EngineRequest =
  | { readonly kind: 'open'; readonly bytes: Uint8Array }
  | { readonly kind: 'renderPage'; readonly pageIndex: number; readonly dpi: number };

export interface EngineResults {
  open: DocumentInfo;
  renderPage: RenderedPage;
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
  | { readonly type: 'error'; readonly id: number; readonly message: string };

/** The slice of `Worker` / `DedicatedWorkerGlobalScope` both ends talk through. */
export interface MessagePortLike<In, Out> {
  postMessage(message: Out, transfer?: Transferable[]): void;
  addEventListener(type: 'message', listener: (event: MessageEvent<In>) => void): void;
}
