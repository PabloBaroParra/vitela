// Opening PDFs from the OS with the installed app (T-266): the manifest
// declares Vitela a handler for PDFs, and Chromium hands the launched files
// to the page's `launchQueue`. The launch is simulated (e2e/launch.ts).
// Chromium only: Firefox and WebKit have no launchQueue and no file_handlers,
// and their file handles cannot be written back to.

import { expect, test } from '@playwright/test';
import { fakeLaunchQueue, launchFiles, launchedFileHead } from './launch';
import { buildPdf } from './pdf';
import { expectOpened, loadApp } from './support';

const PDF = buildPdf([{ widthPt: 612, heightPt: 792, lines: ['launched from the OS'] }]);

test('the manifest offers the app for PDFs, each launch in a new window', async ({ request }) => {
  const manifest = await (await request.get('/manifest.webmanifest')).json();
  expect(manifest.file_handlers).toEqual([{ action: '/', accept: { 'application/pdf': ['.pdf'] } }]);
  expect(manifest.launch_handler).toEqual({ client_mode: 'navigate-new' });
});

test('a launched PDF opens as a picked one, and Save writes back over the launched file', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'launchQueue and writable file handles are Chromium-only');
  await fakeLaunchQueue(page);
  await loadApp(page);

  // Two files launched together: the PDF opens, as with a drop of both.
  await launchFiles(page, [
    { name: 'notes.txt', bytes: new TextEncoder().encode('not a pdf') },
    { name: 'launched.pdf', bytes: PDF },
  ]);
  await expectOpened(page, 'launched.pdf');

  // The launched file's handle came along: Save goes over it, not to a download.
  const downloads: string[] = [];
  page.on('download', (download) => downloads.push(download.suggestedFilename()));
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByText('Saved over launched.pdf.')).toBeVisible({ timeout: 30_000 });
  expect(await launchedFileHead(page, 'launched.pdf', 5)).toBe('%PDF-');
  expect(downloads).toEqual([]);
});
