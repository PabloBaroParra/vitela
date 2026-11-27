// A simulated OS launch of the installed app (T-266), for Chromium.
//
// A real launch needs an installed app and the OS, which Playwright cannot
// drive. Instead an init script puts a fake `window.launchQueue` in place
// before the app loads, and `launchFiles` hands the consumer the app set real
// FileSystemFileHandles, written to the origin private file system first.

import type { Page } from '@playwright/test';

declare global {
  interface Window {
    /** The consumer the app gave the fake launch queue. */
    __launch?: (params: { files: FileSystemFileHandle[] }) => void;
  }
}

/** Replaces the browser's launch queue before the app loads. Call before `loadApp`. */
export async function fakeLaunchQueue(page: Page): Promise<void> {
  await page.addInitScript(() => {
    Object.defineProperty(window, 'launchQueue', {
      configurable: true,
      value: { setConsumer: (consumer: Window['__launch']) => void (window.__launch = consumer) },
    });
  });
}

/** Writes each file to the origin private file system and launches the app with all of them at once. */
export async function launchFiles(page: Page, files: readonly { name: string; bytes: Uint8Array }[]): Promise<void> {
  await page.evaluate(async (entries) => {
    const root = await navigator.storage.getDirectory();
    const handles: FileSystemFileHandle[] = [];
    for (const { name, bytes } of entries) {
      const handle = await root.getFileHandle(name, { create: true });
      const writable = await handle.createWritable();
      await writable.write(new Uint8Array(bytes));
      await writable.close();
      handles.push(handle);
    }
    window.__launch!({ files: handles });
  }, files.map(({ name, bytes }) => ({ name, bytes: [...bytes] })));
}

/** The first bytes of `name` in the origin private file system, as text. */
export function launchedFileHead(page: Page, name: string, length: number): Promise<string> {
  return page.evaluate(
    async ({ name, length }) => {
      const file = await (await (await navigator.storage.getDirectory()).getFileHandle(name)).getFile();
      return new TextDecoder().decode(await file.slice(0, length).arrayBuffer());
    },
    { name, length },
  );
}
