// The editor as a consumer installs it (the packed tarballs): open a PDF from
// a URL, edit it (delete a page in Organize), save, and receive the bytes in
// the host's `onSave`; then reopen those bytes to see the edit is in them.
// Once with <VitelaViewer> (index.html), once with mountViewer (vanilla.html).

import { expect, test, type Page } from '@playwright/test';

/** The sample (public/sample.pdf, written by `npm run setup`) has ten pages. */
const PAGES = 10;

async function expectOpened(page: Page, name: string, pages: number): Promise<void> {
  await expect(page.getByText(`${name} — ${pages} pages`)).toBeVisible({ timeout: 30_000 });
  const slot = page.locator('.page-slot[data-page="1"]');
  await expect(slot).toHaveAttribute('data-state', 'rendered', { timeout: 30_000 });
  await expect(slot.locator('canvas.page-canvas')).toHaveAttribute('data-painted', 'true', { timeout: 30_000 });
}

/** Deletes the first page in Organize, the editor's own way. */
async function deleteFirstPage(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'Organize' }).click();
  const cards = page.locator('.page-card');
  await expect(cards).toHaveCount(PAGES, { timeout: 30_000 });
  await cards.first().click();
  await page.getByRole('toolbar', { name: 'Organize' }).getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(cards).toHaveCount(PAGES - 1);
  await expect(page.getByText('Unsaved changes.')).toBeVisible();
}

const SAVED = new RegExp(`^sample\\.pdf: \\d+ bytes, starts %PDF-$`);

test('<VitelaViewer>: opens from a URL, edits, and hands the saved bytes to onSave', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/');
  await expectOpened(page, 'sample.pdf', PAGES);
  // Embedded: the picker is offered by default.
  await expect(page.getByLabel('Open PDF')).toBeAttached();

  await deleteFirstPage(page);
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.getByText('Saved sample.pdf.')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('#saved')).toHaveText(SAVED);
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);

  // The host hands the saved bytes back as the source: the deletion is in them.
  await page.getByRole('button', { name: 'Open the saved copy' }).click();
  await expect(page.getByText(`saved.pdf — ${PAGES - 1} pages`)).toBeVisible({ timeout: 30_000 });
  expect(errors).toEqual([]);
});

test('mountViewer: opens from a URL, edits, saves to onSave, reopens the bytes, and destroy stops the engine', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/vanilla.html');
  await expectOpened(page, 'sample.pdf', PAGES);
  // This host asked for no picker.
  await expect(page.getByLabel('Open PDF')).toHaveCount(0);

  await deleteFirstPage(page);
  await page.evaluate(() => window.viewer.save());
  await expect(page.locator('#saved')).toHaveText(SAVED, { timeout: 30_000 });
  expect(await page.evaluate(() => window.saved.length)).toBe(1);

  await page.evaluate(() => window.viewer.open(window.saved[0]!.bytes, 'saved.pdf'));
  await expectOpened(page, 'saved.pdf', PAGES - 1);

  // The editor started its own engine, so destroy stops its worker.
  expect(page.workers()).toHaveLength(1);
  const closed = page.workers()[0]!.waitForEvent('close');
  await page.evaluate(() => window.viewer.destroy());
  await expect(page.locator('#editor')).toBeEmpty();
  await closed;
  expect(page.workers()).toHaveLength(0);
  expect(errors).toEqual([]);
});
