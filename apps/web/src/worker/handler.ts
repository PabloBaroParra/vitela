// Dispatches one request from the UI to the engine and shapes the reply.
// Pure apart from the engine it is given, so it is tested with a fake one.

import { errorKind, errorMessage } from '../core/errors';
import type { RequestEnvelope, WorkerMessage } from '../core/protocol';
import type { Engine } from '../core/types';

export interface Reply {
  readonly message: WorkerMessage;
  readonly transfer: Transferable[];
}

export async function handleRequest(engine: Engine, { id, request }: RequestEnvelope): Promise<Reply> {
  try {
    switch (request.kind) {
      case 'open':
        return {
          message: { type: 'result', id, value: await engine.open(request.bytes, request.password) },
          transfer: [],
        };
      case 'renderPage': {
        const page = await engine.renderPage(request.document, request.pageIndex, request.dpi);
        return { message: { type: 'result', id, value: page }, transfer: [page.rgba.buffer as ArrayBuffer] };
      }
      case 'close':
        await engine.close(request.document);
        return { message: { type: 'result', id, value: undefined }, transfer: [] };
    }
  } catch (error) {
    // The core names every failure kind (`WrongPassword`, ...): the UI
    // branches on that name, so it must cross the worker boundary too.
    return { message: { type: 'error', id, name: errorKind(error) ?? 'Error', message: errorMessage(error) }, transfer: [] };
  }
}
