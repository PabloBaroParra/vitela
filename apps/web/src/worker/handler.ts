// Dispatches one request from the UI to the engine and shapes the reply.
// Pure apart from the engine it is given, so it is tested with a fake one.

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
        return { message: { type: 'result', id, value: await engine.open(request.bytes) }, transfer: [] };
      case 'renderPage': {
        const page = await engine.renderPage(request.pageIndex, request.targetWidth);
        return { message: { type: 'result', id, value: page }, transfer: [page.rgba.buffer as ArrayBuffer] };
      }
    }
  } catch (error) {
    return { message: { type: 'error', id, message: errorMessage(error) }, transfer: [] };
  }
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
