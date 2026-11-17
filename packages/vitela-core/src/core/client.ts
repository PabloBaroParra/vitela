// Typed client for the engine worker. The UI talks to the engine only
// through this class; the worker behind it is an implementation detail.

import type {
  EngineRequest,
  EngineResults,
  MessagePortLike,
  RequestEnvelope,
  WorkerMessage,
} from './protocol.js';
import { EngineError } from './errors.js';
import type {
  DocumentHandle,
  EditedDocument,
  Engine,
  OpenedDocument,
  PageEdit,
  PagePoint,
  RenderedPage,
  SearchHit,
  TextSelection,
} from './types.js';

interface Pending {
  resolve(value: unknown): void;
  reject(reason: Error): void;
}

export class EngineClient implements Engine {
  /** Settles once the worker has loaded both WASM modules (or failed to). */
  readonly ready: Promise<void>;

  private readonly pending = new Map<number, Pending>();
  private nextId = 1;
  private failure: Error | null = null;
  private settleReady!: { resolve(): void; reject(reason: Error): void };

  constructor(private readonly port: MessagePortLike<WorkerMessage, RequestEnvelope>) {
    this.ready = new Promise<void>((resolve, reject) => {
      this.settleReady = { resolve, reject };
    });
    // A rejected `ready` nobody awaited yet must not surface as unhandled.
    this.ready.catch(() => {});
    port.addEventListener('message', (event) => this.receive(event.data));
  }

  open(bytes: Uint8Array, password?: string): Promise<OpenedDocument> {
    // The bytes are transferred, not copied: the caller's buffer is detached.
    return this.send({ kind: 'open', bytes, password }, [bytes.buffer as ArrayBuffer]);
  }

  renderPage(document: DocumentHandle, pageIndex: number, dpi: number): Promise<RenderedPage> {
    return this.send({ kind: 'renderPage', document, pageIndex, dpi });
  }

  close(document: DocumentHandle): Promise<void> {
    return this.send({ kind: 'close', document });
  }

  saveToBytes(document: DocumentHandle, invalidateSignatures: boolean): Promise<Uint8Array> {
    return this.send({ kind: 'save', document, invalidateSignatures });
  }

  applyEdit(document: DocumentHandle, edit: PageEdit): Promise<EditedDocument> {
    return this.send({ kind: 'applyEdit', document, edit });
  }

  undo(document: DocumentHandle): Promise<EditedDocument> {
    return this.send({ kind: 'undo', document });
  }

  redo(document: DocumentHandle): Promise<EditedDocument> {
    return this.send({ kind: 'redo', document });
  }

  search(document: DocumentHandle, query: string): Promise<SearchHit[]> {
    return this.send({ kind: 'search', document, query });
  }

  selectText(document: DocumentHandle, pageIndex: number, anchor: PagePoint, focus: PagePoint): Promise<TextSelection> {
    return this.send({ kind: 'selectText', document, pageIndex, anchor, focus });
  }

  selectWord(document: DocumentHandle, pageIndex: number, point: PagePoint): Promise<TextSelection> {
    return this.send({ kind: 'selectWord', document, pageIndex, point });
  }

  /** Marks the engine dead (e.g. the worker script failed to load). */
  fail(message: string): void {
    if (this.failure) return;
    this.failure = new Error(message);
    this.settleReady.reject(this.failure);
    for (const { reject } of this.pending.values()) reject(this.failure);
    this.pending.clear();
  }

  private send<K extends EngineRequest['kind']>(
    request: Extract<EngineRequest, { kind: K }>,
    transfer: Transferable[] = [],
  ): Promise<EngineResults[K]> {
    if (this.failure) return Promise.reject(this.failure);
    const id = this.nextId++;
    return new Promise<EngineResults[K]>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
      this.port.postMessage({ id, request }, transfer);
    });
  }

  private receive(message: WorkerMessage): void {
    switch (message.type) {
      case 'ready':
        this.settleReady.resolve();
        return;
      case 'fatal':
        this.fail(message.message);
        return;
      case 'result':
      case 'error': {
        const pending = this.pending.get(message.id);
        if (!pending) return;
        this.pending.delete(message.id);
        if (message.type === 'result') pending.resolve(message.value);
        else pending.reject(new EngineError(message.name, message.message));
      }
    }
  }
}
