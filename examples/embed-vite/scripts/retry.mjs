// Waiting out npm's propagation: a version just published can take minutes to
// be served everywhere.

/** Whether npm failed because the registry does not serve that package or version (yet). */
export function notPropagatedYet({ status, stderr }) {
  return status !== 0 && /\bcode (E404|ETARGET)\b/.test(stderr);
}

/** Runs `attempt` up to `attempts` times while `again(result)`; resolves to the last result. */
export async function retry(attempt, { attempts, delayMs, again, wait = (ms) => new Promise((done) => setTimeout(done, ms)) }) {
  for (let left = attempts - 1; ; left -= 1) {
    const result = attempt();
    if (left <= 0 || !again(result)) return result;
    await wait(delayMs);
  }
}
