// The browser's own "leave site?" warning (T-267): asked for while the open
// document has unsaved edits, so a close or reload does not lose them
// silently; and not asked for before any edit, nor once the document is
// clean again. Browsers show the warning only after the user has interacted
// with the page: the edit below is made with real clicks.
//
// Saving to a download leaves the document dirty (the edits are not where it
// lives), so "saved" is checked in Chromium, saving over a launched file's
// handle (e2e/launch.ts); every browser checks a fresh document instead.
//
// Playwright's WebKit never closes a page on `close({ runBeforeUnload: true })`
// when nothing asks, so there the clean case is checked by reloading only.

import { expect, test, type Dialog, type Page } from '@playwright/test';
import { fakeLaunchQueue, launchFiles } from './launch';
import { cards, openOrganize, saveButton, tool } from './organize';
import { buildPdf } from './pdf';
import { expectOpened, loadApp, openBytes } from './support';

const TWO_PAGES = buildPdf([
  { widthPt: 612, heightPt: 792 },
  { widthPt: 612, heightPt: 792 },
]);

/** Deletes page 1 in Organize: one edit, unsaved. */
async function makeAnEdit(page: Page) {
  await openOrganize(page);
  await cards(page).nth(0).click();
  await tool(page, 'Delete').click();
  await expect(page.getByText('Unsaved changes.')).toBeVisible();
}

/** Every dialog the page opens from now on, dismissed so nothing waits on it. */
function recordDialogs(page: Page): Dialog[] {
  const dialogs: Dialog[] = [];
  page.on('dialog', (dialog) => {
    dialogs.push(dialog);
    void dialog.dismiss().catch(() => {});
  });
  return dialogs;
}

/** Reloads, expecting no warning, and waits for the app to be back. */
async function reloadWithoutAsking(page: Page) {
  const dialogs = recordDialogs(page);
  await page.reload();
  await expect(page.locator('main.app')).toHaveAttribute('data-engine', 'ready', { timeout: 30_000 });
  expect(dialogs.map((dialog) => dialog.type())).toEqual([]);
}

test('a clean document reloads and closes without a warning', async ({ page, browserName }) => {
  await loadApp(page);
  await openBytes(page, 'clean.pdf', TWO_PAGES);
  // Interacted with, but nothing edited.
  await openOrganize(page);
  await reloadWithoutAsking(page);

  if (browserName === 'webkit') return;
  const dialogs = recordDialogs(page);
  await page.mouse.click(10, 10);
  await page.close({ runBeforeUnload: true });
  await expect.poll(() => page.isClosed()).toBe(true);
  expect(dialogs.map((dialog) => dialog.type())).toEqual([]);
});

test('unsaved edits make a reload ask first', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'edited.pdf', TWO_PAGES);
  await makeAnEdit(page);

  const asked = page.waitForEvent('dialog');
  const reload = page.reload();
  const dialog = await asked;
  expect(dialog.type()).toBe('beforeunload');
  // Leaving anyway reloads the app, the edits gone.
  await dialog.accept();
  await reload;
  await expect(page.locator('main.app')).toHaveAttribute('data-engine', 'ready', { timeout: 30_000 });
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);
});

test('unsaved edits make a close ask first, staying keeps them, and a fresh document stops asking', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'kept.pdf', TWO_PAGES);
  await makeAnEdit(page);

  const asked = page.waitForEvent('dialog');
  await page.close({ runBeforeUnload: true });
  const dialog = await asked;
  expect(dialog.type()).toBe('beforeunload');
  await dialog.dismiss();
  // Stayed: the page and its edits are still there.
  expect(page.isClosed()).toBe(false);
  await expect(page.getByText('Unsaved changes.')).toBeVisible();

  // Another file opened over it starts clean: nothing left to warn about.
  await openBytes(page, 'fresh.pdf', TWO_PAGES);
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);
  await reloadWithoutAsking(page);
});

test('edits saved over the opened file stop the warning', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'saving over the opened file needs a writable file handle (Chromium-only)');
  await fakeLaunchQueue(page);
  await loadApp(page);
  await launchFiles(page, [{ name: 'saved.pdf', bytes: TWO_PAGES }]);
  await expectOpened(page, 'saved.pdf');
  await makeAnEdit(page);

  await saveButton(page).click();
  await expect(page.getByText('Saved over saved.pdf.')).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);
  await reloadWithoutAsking(page);
});
