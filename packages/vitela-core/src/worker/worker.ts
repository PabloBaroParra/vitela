// Web Worker entry: wires the worker's global scope to the engine's message
// loop (serve.ts), with the real loader and this build's asset digests.

import type { ClientMessage, MessagePortLike, WorkerMessage } from '../core/protocol.js';
import { ASSET_SHA256 } from '../generated/integrity.js';
import { loadEngine } from './engine.js';
import { serveEngine } from './serve.js';

serveEngine(self as unknown as MessagePortLike<ClientMessage, WorkerMessage>, loadEngine, ASSET_SHA256);
