// Compress (T-250): the dialog, the real core's compressed save, and the
// smaller copy as a download — parity with the native Compress tiles. A copy
// is offered only when it came out smaller; an encrypted document is refused
// in the dialog before anything runs; a signed one asks, and "Compress
// anyway" is the consent.

import { readFileSync } from 'node:fs';
import { expect, test, type Download, type Locator, type Page } from '@playwright/test';
import { cards, openOrganize, tool } from './organize';
import { expectOpened, fixtureBytes, inkedPixelCount, loadApp, openBytes, openFixture } from './support';

const SCAN = 'compress/scan_200dpi.pdf';
const THREE_PAGES = 'compress/vector_only.pdf';
const SIGNED = 'signed/rsa2048_sha256.pdf';
// tests/fixtures/README.md, encrypted/ corpus.
const ENCRYPTED = 'encrypted/aes_128_user_and_owner.pdf';
const USER_PASSWORD = 'user-aes-pass';

/** The workspace bar's Compress button (the dialog has one of its own). */
const compressButton = (page: Page) => page.locator('.workspace__bar').getByRole('button', { name: 'Compress', exact: true });
const compressDialog = (page: Page) => page.getByRole('dialog', { name: 'Compress PDF' });

async function openDialog(page: Page): Promise<Locator> {
  await compressButton(page).click();
  const dialog = compressDialog(page);
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  return dialog;
}

/** Clicks `confirm` in the dialog and returns the download it starts. */
async function compressToDownload(page: Page, confirm: Locator): Promise<{ download: Download; bytes: Buffer }> {
  const [download] = await Promise.all([page.waitForEvent('download', { timeout: 60_000 }), confirm.click()]);
  return { download, bytes: readFileSync(await download.path()) };
}

test('a scan compresses to a smaller copy that downloads and opens', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, SCAN);

  const dialog = await openDialog(page);
  await expect(dialog.getByRole('radio')).toHaveCount(3);
  await expect(dialog.getByRole('radio', { name: /^Balanced/ })).toBeChecked();
  await dialog.getByRole('radio', { name: /^Small/ }).check();

  const { download, bytes } = await compressToDownload(page, dialog.getByRole('button', { name: 'Compress', exact: true }));
  expect(download.suggestedFilename()).toBe('scan_200dpi-compressed.pdf');
  expect(bytes.subarray(0, 5).toString('latin1')).toBe('%PDF-');
  expect(bytes.length).toBeLessThan(fixtureBytes(SCAN).length);
  await expect(compressDialog(page)).toHaveCount(0);
  await expect(page.getByText(/uncompressed → .* compressed \(\d+% smaller\)\. Downloaded scan_200dpi-compressed\.pdf\./)).toBeVisible();
  // The open document is untouched: nothing to save.
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);

  await openBytes(page, 'smaller.pdf', bytes);
  await expect(page.getByText('smaller.pdf — 1 page')).toBeVisible();
  expect((await inkedPixelCount(page)).inked).toBeGreaterThan(0);
});

test('the copy carries the pending edits, and the edits stay unsaved and undoable', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, THREE_PAGES);
  await openOrganize(page);
  await cards(page).nth(0).click();
  await tool(page, 'Delete').click();
  await expect(cards(page)).toHaveCount(2);

  const dialog = await openDialog(page);
  await dialog.getByRole('radio', { name: /^Lossless/ }).check();
  const { bytes } = await compressToDownload(page, dialog.getByRole('button', { name: 'Compress', exact: true }));
  await expect(page.getByText('Unsaved changes.')).toBeVisible();
  await expect(tool(page, 'Undo')).toHaveAttribute('aria-disabled', 'false');

  await page.getByRole('button', { name: 'View' }).click();
  await openBytes(page, 'two.pdf', bytes);
  await expect(page.getByText('two.pdf — 2 pages')).toBeVisible();
});

test('an encrypted document is refused in the dialog and nothing is written', async ({ page }) => {
  await loadApp(page);
  await page.getByLabel('Open PDF').setInputFiles({ name: 'locked.pdf', mimeType: 'application/pdf', buffer: fixtureBytes(ENCRYPTED) });
  const password = page.getByRole('dialog', { name: 'Password required' });
  await expect(password).toBeVisible({ timeout: 30_000 });
  await password.getByLabel('Password').fill(USER_PASSWORD);
  await password.getByLabel('Password').press('Enter');
  await expectOpened(page, 'locked.pdf');

  let downloads = 0;
  page.on('download', () => downloads++);
  const dialog = await openDialog(page);
  await expect(dialog.getByRole('alert')).toHaveText(/password protection does not allow it to be rewritten, so it cannot be compressed\.$/);
  await expect(dialog.getByRole('button', { name: 'Compress', exact: true })).toBeDisabled();
  for (const radio of await dialog.getByRole('radio').all()) await expect(radio).toBeDisabled();
  await page.keyboard.press('Escape');
  await expect(compressDialog(page)).toHaveCount(0);
  await expect(compressButton(page)).toBeFocused();
  expect(downloads).toBe(0);
});

test('a signed document asks first, and "Compress anyway" downloads the copy', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, SIGNED);

  const dialog = await openDialog(page);
  await expect(dialog.getByText(/This document is signed\./)).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Compress', exact: true })).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(compressDialog(page)).toHaveCount(0);

  const again = await openDialog(page);
  const { download, bytes } = await compressToDownload(page, again.getByRole('button', { name: 'Compress anyway' }));
  expect(download.suggestedFilename()).toBe('rsa2048_sha256-compressed.pdf');
  expect(bytes.length).toBeLessThan(fixtureBytes(SIGNED).length);
  await openBytes(page, 'unsigned-copy.pdf', bytes);
});
