import { test } from 'node:test';
import assert from 'node:assert/strict';
import { notPropagatedYet, retry } from './retry.mjs';

const lag = { status: 1, stderr: 'npm error code ETARGET' };
const ok = { status: 0, stderr: '' };

async function run(results, attempts = 3) {
  const waits = [];
  let calls = 0;
  const result = await retry(() => results[calls++], {
    attempts,
    delayMs: 5,
    again: notPropagatedYet,
    wait: async (ms) => void waits.push(ms),
  });
  return { result, calls, waits };
}

test('a first success neither waits nor retries', async () => {
  assert.deepEqual(await run([ok]), { result: ok, calls: 1, waits: [] });
});

test('propagation lag is retried until it clears', async () => {
  assert.deepEqual(await run([lag, lag, ok]), { result: ok, calls: 3, waits: [5, 5] });
});

test('the retries are bounded, and the last failure is the result', async () => {
  assert.deepEqual(await run([lag, lag, lag, ok]), { result: lag, calls: 3, waits: [5, 5] });
});

test('any other failure is not retried', async () => {
  const other = { status: 1, stderr: 'npm error code ERESOLVE' };
  assert.deepEqual(await run([other, ok]), { result: other, calls: 1, waits: [] });
});

test('lag is a package or version the registry does not serve yet', () => {
  assert.ok(notPropagatedYet({ status: 1, stderr: 'npm error code E404\n' }));
  assert.ok(notPropagatedYet(lag));
  assert.ok(!notPropagatedYet({ status: 1, stderr: 'npm error code EINTEGRITY' }));
  assert.ok(!notPropagatedYet({ status: 0, stderr: 'npm error code E404' }));
  assert.ok(!notPropagatedYet({ status: null, stderr: '' }));
});
