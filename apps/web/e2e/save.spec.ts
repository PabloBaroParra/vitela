// Saving: a download in every browser, and — in Chromium, the only browser
// with the File System Access API — a write over the file the user opened.
//
// Playwright cannot drive the native file picker, so the Chromium flows hand
// the app a REAL FileSystemFileHandle from the origin-private file system
// (`navigator.storage.getDirectory()`), through the same two doors a user's
// file comes in by: `showOpenFilePicker` and a drop's `getAsFileSystemHandle`.
// The app then writes through the real `createWritable()`. What stays
// untested is the browser's own permission prompt and a handle onto the real
// disk (see apps/web/README.md).

import { readFileSync } from 'node:fs';
import { expect, test, type Download, type Page } from '@playwright/test';
import { dropFile, expectOpened, fixtureBytes, inkedPixelCount, loadApp, openBytes, openFixture } from './support';

const THREE_PAGES = 'compress/vector_only.pdf';
const SIGNED = 'signed/rsa2048_sha256.pdf';
// tests/fixtures/README.md, encrypted/ corpus.
const ENCRYPTED = 'encrypted/aes_128_user_and_owner.pdf';
const USER_PASSWORD = 'user-aes-pass';

const saveButton = (page: Page) => page.getByRole('button', { name: 'Save', exact: true });

/** Clicks Save and returns the download it starts. */
async function saveAsDownload(page: Page): Promise<{ download: Download; bytes: Buffer }> {
  const [download] = await Promise.all([page.waitForEvent('download'), saveButton(page).click()]);
  return { download, bytes: readFileSync(await download.path()) };
}

test.describe('download', () => {
  test('saves the open document as a download that reopens with the same pages', async ({ page }) => {
    await loadApp(page);
    await openFixture(page, THREE_PAGES);

    const { download, bytes } = await saveAsDownload(page);
    expect(download.suggestedFilename()).toBe('vector_only.pdf');
    expect(bytes.subarray(0, 5).toString('latin1')).toBe('%PDF-');
    await expect(page.getByText('Downloaded vector_only.pdf.')).toBeVisible();

    await openBytes(page, 'saved.pdf', bytes);
    await expect(page.getByText('saved.pdf — 3 pages')).toBeVisible();
    expect((await inkedPixelCount(page)).inked).toBeGreaterThan(0);
  });

  test('a dropped file, which carries no handle here, is saved by download too', async ({ page }) => {
    await loadApp(page);
    await dropFile(page, 'dropped.pdf', fixtureBytes('forms/reportlab_acroform.pdf'));
    await expectOpened(page, 'dropped.pdf');

    const { download, bytes } = await saveAsDownload(page);
    expect(download.suggestedFilename()).toBe('dropped.pdf');
    await openBytes(page, 'again.pdf', bytes);
    await expect(page.getByText('again.pdf — 1 page')).toBeVisible();
  });

  test('an encrypted document saves still encrypted: the same password reopens it', async ({ page }) => {
    await loadApp(page);
    await page.getByLabel('Open PDF').setInputFiles({
      name: 'locked.pdf',
      mimeType: 'application/pdf',
      buffer: fixtureBytes(ENCRYPTED),
    });
    const dialog = page.getByRole('dialog', { name: 'Password required' });
    await expect(dialog).toBeVisible({ timeout: 30_000 });
    await dialog.getByLabel('Password').fill(USER_PASSWORD);
    await dialog.getByLabel('Password').press('Enter');
    await expectOpened(page, 'locked.pdf');

    const { bytes } = await saveAsDownload(page);
    expect(bytes.toString('latin1')).toContain('/Encrypt');

    await page.getByLabel('Open PDF').setInputFiles({ name: 'resaved.pdf', mimeType: 'application/pdf', buffer: bytes });
    await expect(dialog).toBeVisible({ timeout: 30_000 });
    await dialog.getByLabel('Password').fill(USER_PASSWORD);
    await dialog.getByLabel('Password').press('Enter');
    await expectOpened(page, 'resaved.pdf');
  });

  test('an unedited signed document saves without the signature question, signature bytes intact', async ({ page }) => {
    // Nothing is rewritten without an edit, so there is no signature to break.
    // The question itself needs an edit (T-226): it is covered by the
    // SaveControls and useSave Vitest suites, not here.
    await loadApp(page);
    await openFixture(page, SIGNED);
    const { bytes } = await saveAsDownload(page);
    await expect(page.getByRole('dialog')).toHaveCount(0);
    expect(bytes.subarray(0, 5).toString('latin1')).toBe('%PDF-');
    expect(bytes.subarray(0, fixtureBytes(SIGNED).length).equals(fixtureBytes(SIGNED))).toBe(true);
    await openBytes(page, 'was-signed.pdf', bytes);
  });
});

test.describe('File System Access (Chromium)', () => {
  test.skip(({ browserName }) => browserName !== 'chromium', 'only Chromium has the File System Access API');

  /** Puts `bytes` in the origin-private file system as `name`; the handle is kept on `window`. */
  async function stageHandle(page: Page, name: string, bytes: Buffer): Promise<void> {
    await page.evaluate(
      async ({ name, bytes }) => {
        const root = await navigator.storage.getDirectory();
        const handle = await root.getFileHandle(name, { create: true });
        const writable = await handle.createWritable();
        await writable.write(new Uint8Array(bytes));
        await writable.close();
        (window as unknown as { __staged: FileSystemFileHandle }).__staged = handle;
      },
      { name, bytes: [...bytes] },
    );
  }

  /**
   * Replaces the staged file's content with a marker, once the app has read
   * it. An unedited save can reproduce the original bytes, so without this a
   * test could not tell "written over" from "left alone".
   */
  async function clobber(page: Page): Promise<void> {
    await page.evaluate(async () => {
      const handle = (window as unknown as { __staged: FileSystemFileHandle }).__staged;
      const writable = await handle.createWritable();
      await writable.write('stale');
      await writable.close();
    });
    expect(Buffer.from(await stagedBytes(page)).toString('latin1')).toBe('stale');
  }

  /** What the staged file holds now. */
  const stagedBytes = (page: Page): Promise<number[]> =>
    page.evaluate(async () => {
      const handle = (window as unknown as { __staged: FileSystemFileHandle }).__staged;
      return [...new Uint8Array(await (await handle.getFile()).arrayBuffer())];
    });

  test('Open PDF uses the browser picker, and Save writes over that very file', async ({ page }) => {
    await loadApp(page);
    const original = fixtureBytes(THREE_PAGES);
    await stageHandle(page, 'picked.pdf', original);
    // Stand in for the native dialog Playwright cannot drive.
    await page.evaluate(() => {
      const w = window as unknown as { __staged: FileSystemFileHandle; showOpenFilePicker: () => Promise<unknown[]> };
      w.showOpenFilePicker = async () => [w.__staged];
    });
    let downloads = 0;
    page.on('download', () => downloads++);

    await page.locator('.file-picker').click();
    await expectOpened(page, 'picked.pdf');
    await clobber(page);
    await saveButton(page).click();
    await expect(page.getByText('Saved over picked.pdf.')).toBeVisible();

    const written = Buffer.from(await stagedBytes(page));
    expect(written.subarray(0, 5).toString('latin1')).toBe('%PDF-');
    expect(downloads).toBe(0);

    // The file the app wrote reopens with the same pages.
    await openBytes(page, 'reopened.pdf', written);
    await expect(page.getByText('reopened.pdf — 3 pages')).toBeVisible();
  });

  test('a file dropped with its handle is saved over too', async ({ page }) => {
    await loadApp(page);
    await stageHandle(page, 'dropped.pdf', fixtureBytes('forms/reportlab_acroform.pdf'));
    await page.evaluate(() => {
      const staged = (window as unknown as { __staged: FileSystemFileHandle }).__staged;
      // A synthetic DataTransfer has no handles: give its items the staged one.
      (DataTransferItem.prototype as unknown as { getAsFileSystemHandle: () => Promise<unknown> }).getAsFileSystemHandle =
        async () => staged;
    });
    let downloads = 0;
    page.on('download', () => downloads++);

    await dropFile(page, 'dropped.pdf', fixtureBytes('forms/reportlab_acroform.pdf'));
    await expectOpened(page, 'dropped.pdf');
    await clobber(page);
    await saveButton(page).click();
    await expect(page.getByText('Saved over dropped.pdf.')).toBeVisible();

    const written = Buffer.from(await stagedBytes(page));
    expect(written.subarray(0, 5).toString('latin1')).toBe('%PDF-');
    expect(downloads).toBe(0);
    await openBytes(page, 'back.pdf', written);
    await expect(page.getByText('back.pdf — 1 page')).toBeVisible();
  });

  test('downloads instead, and says so, when the handle cannot be written', async ({ page }) => {
    await loadApp(page);
    await stageHandle(page, 'locked-down.pdf', fixtureBytes(THREE_PAGES));
    await page.evaluate(() => {
      const w = window as unknown as { __staged: FileSystemFileHandle; showOpenFilePicker: () => Promise<unknown[]> };
      w.__staged.createWritable = async () => {
        throw new DOMException('read-only', 'NoModificationAllowedError');
      };
      w.showOpenFilePicker = async () => [w.__staged];
    });

    await page.locator('.file-picker').click();
    await expectOpened(page, 'locked-down.pdf');
    const { download } = await saveAsDownload(page);
    expect(download.suggestedFilename()).toBe('locked-down.pdf');
    await expect(
      page.getByText('Could not write over the opened file, so locked-down.pdf was downloaded instead.'),
    ).toBeVisible();
  });
});
