// Organize: reorder (pointer drag and keyboard), rotate, delete, undo/redo —
// then save and reopen the saved bytes to check what the PDF now holds.
//
// Every page of the built document has its own width, so a page is known by
// its shape: the grid draws each thumbnail at its page's aspect ratio, and a
// turned page swaps it. Reopening the download and reading the shapes back
// checks the order, the turn and the count the core actually wrote.

import { readFileSync } from 'node:fs';
import { expect, test, type Download, type Locator, type Page } from '@playwright/test';
import { buildPdf } from './pdf';
import { fixtureBytes, loadApp, openBytes, openFixture } from './support';

/** Four portrait pages told apart by their width, all 700 pt tall. */
const WIDTHS = [300, 400, 500, 600] as const;
const HEIGHT = 700;
const SHAPES = buildPdf(WIDTHS.map((widthPt) => ({ widthPt, heightPt: HEIGHT })));

/** `/P` without bit 4 (modify) and bit 11 (assembly): the pages may not be reorganized. */
const ASSEMBLY_PROHIBITED = -4 & ~(1 << 3) & ~(1 << 10);
const SIGNED = 'signed/rsa2048_sha256.pdf';

const cards = (page: Page) => page.locator('.page-card');
const grid = (page: Page) => page.getByRole('listbox', { name: /^Pages of / });
const toolbar = (page: Page) => page.getByRole('toolbar', { name: 'Organize' });
const tool = (page: Page, name: string) => toolbar(page).getByRole('button', { name, exact: true });

async function openOrganize(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'Organize' }).click();
  await expect(grid(page)).toBeVisible();
  await expectThumbnailsRendered(page);
}

async function expectThumbnailsRendered(page: Page): Promise<void> {
  await expect(cards(page).first()).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.page-card:not([data-state="rendered"])')).toHaveCount(0, { timeout: 30_000 });
}

/**
 * Each card's page as `width` pt, or `width r` when it is turned a quarter:
 * the width read back from the thumbnail's drawn aspect ratio (pages are
 * all 700 pt tall before turning).
 */
async function shapes(page: Page): Promise<string[]> {
  const boxes = await page.locator('.page-card').evaluateAll((nodes) =>
    nodes.map((node) => {
      const box = node.querySelector<HTMLElement>('.page-card__page')!.getBoundingClientRect();
      return { ratio: box.width / box.height, rotation: Number((node as HTMLElement).dataset.rotation) };
    }),
  );
  return boxes.map(({ ratio, rotation }) => {
    const turned = rotation % 180 !== 0;
    // A turned page is drawn HEIGHT wide: its own width is now its height.
    const width = Math.round((turned ? HEIGHT / ratio : HEIGHT * ratio) / 100) * 100;
    return turned ? `${width} r` : `${width}`;
  });
}

const saveButton = (page: Page) => page.getByRole('button', { name: 'Save', exact: true });

async function saveAsDownload(page: Page): Promise<{ download: Download; bytes: Buffer }> {
  const [download] = await Promise.all([page.waitForEvent('download'), saveButton(page).click()]);
  return { download, bytes: readFileSync(await download.path()) };
}

/** Drags `from` so it drops `dx` px right of the middle of `onto`. */
async function drag(page: Page, from: Locator, onto: Locator, dx: number): Promise<void> {
  const start = (await from.boundingBox())!;
  const end = (await onto.boundingBox())!;
  await page.mouse.move(start.x + start.width / 2, start.y + start.height / 2);
  await page.mouse.down();
  await page.mouse.move(start.x + start.width / 2 + 20, start.y + start.height / 2, { steps: 4 });
  await page.mouse.move(end.x + end.width / 2 + dx, end.y + end.height / 2, { steps: 8 });
  await page.mouse.up();
}

test('drag, rotate and delete pages, then save: the PDF holds the new order, turn and count', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'shapes.pdf', SHAPES);
  await openOrganize(page);
  expect(await shapes(page)).toEqual(['300', '400', '500', '600']);

  // Page 1 dropped right of page 3's middle: in front of page 4.
  await drag(page, cards(page).nth(0), cards(page).nth(2), 20);
  await expect.poll(() => shapes(page)).toEqual(['400', '500', '300', '600']);
  await expect(page.getByText('Unsaved changes.')).toBeVisible();

  await cards(page).nth(3).click();
  await tool(page, 'Rotate right').click();
  await expect(cards(page).nth(3)).toHaveAttribute('data-rotation', '90');

  await cards(page).nth(0).click();
  await tool(page, 'Delete').click();
  await expect(cards(page)).toHaveCount(3);
  await expectThumbnailsRendered(page);
  expect(await shapes(page)).toEqual(['500', '300', '600 r']);
  await expect(page.getByText('shapes.pdf — 3 pages')).toBeVisible();

  const { download, bytes } = await saveAsDownload(page);
  expect(download.suggestedFilename()).toBe('shapes.pdf');

  await openBytes(page, 'reopened.pdf', bytes);
  await expect(page.getByText('reopened.pdf — 3 pages')).toBeVisible();
  await openOrganize(page);
  expect(await shapes(page)).toEqual(['500', '300', '600 r']);
});

test('reorder with the keyboard, undo and redo, then save', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'shapes.pdf', SHAPES);
  await openOrganize(page);

  await grid(page).focus();
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Shift+ArrowRight');
  await expect(toolbar(page).getByText('Pages 2–3 of 4 selected')).toBeVisible();
  await page.keyboard.press('Control+Shift+ArrowLeft');
  await expect.poll(() => shapes(page)).toEqual(['400', '500', '300', '600']);
  // The selection moved with the pages, so the focus is still on them.
  await expect(toolbar(page).getByText('Pages 1–2 of 4 selected')).toBeVisible();
  await expect(grid(page)).toBeFocused();

  await page.keyboard.press('Delete');
  await expect(cards(page)).toHaveCount(2);
  await page.keyboard.press('Control+z');
  await expect(cards(page)).toHaveCount(4);
  await page.keyboard.press('Control+z');
  await expect.poll(() => shapes(page)).toEqual(['300', '400', '500', '600']);
  await page.keyboard.press('Control+Shift+z');
  await expect.poll(() => shapes(page)).toEqual(['400', '500', '300', '600']);
  await expect(tool(page, 'Redo')).toHaveAttribute('aria-disabled', 'false');

  // The viewer shows the edited document too.
  await page.getByRole('button', { name: 'View' }).click();
  await expect(page.locator('.page-slot[data-page="1"]')).toHaveAttribute('data-state', 'rendered', { timeout: 30_000 });

  const { bytes } = await saveAsDownload(page);
  await openBytes(page, 'reopened.pdf', bytes);
  await openOrganize(page);
  expect(await shapes(page)).toEqual(['400', '500', '300', '600']);
});

test('a PDF that withholds the assembly permission is blocked', async ({ page }) => {
  await loadApp(page);
  const locked = buildPdf(
    WIDTHS.map((widthPt) => ({ widthPt, heightPt: HEIGHT })),
    { encrypt: { permissions: ASSEMBLY_PROHIBITED, userPassword: 'user-pass' } },
  );
  await page.getByLabel('Open PDF').setInputFiles({ name: 'locked.pdf', mimeType: 'application/pdf', buffer: locked });
  const dialog = page.getByRole('dialog', { name: 'Password required' });
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  await dialog.getByLabel('Password').fill('user-pass');
  await dialog.getByLabel('Password').press('Enter');
  await expect(page.getByText('locked.pdf — 4 pages')).toBeVisible({ timeout: 30_000 });

  await openOrganize(page);
  await expect(page.getByText('This PDF does not allow its pages to be moved, rotated or deleted.', { exact: false })).toBeVisible();
  for (const name of ['Rotate left', 'Rotate right', 'Delete', 'Move earlier', 'Move later']) {
    await expect(tool(page, name)).toHaveAttribute('aria-disabled', 'true');
  }

  // Neither a click, nor a key, nor a drag changes anything. (`force`: the
  // button is aria-disabled, which Playwright would otherwise wait out.)
  await tool(page, 'Delete').click({ force: true });
  await grid(page).focus();
  await page.keyboard.press('Delete');
  await page.keyboard.press('Control+Shift+ArrowRight');
  await drag(page, cards(page).nth(0), cards(page).nth(2), 20);
  expect(await shapes(page)).toEqual(['300', '400', '500', '600']);
  await expect(page.getByText('locked.pdf — 4 pages')).toBeVisible();
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);
});

test('a signed document: a turn saves without breaking the signature, a deletion asks, and Save a copy downloads it', async ({
  page,
}) => {
  await loadApp(page);
  await openFixture(page, SIGNED);
  await openOrganize(page);
  const original = fixtureBytes(SIGNED);

  // A quarter-turn stays on the incremental writer: the signed bytes are kept
  // as they were, so there is nothing to ask.
  await tool(page, 'Rotate right').click();
  await expect(cards(page).nth(0)).toHaveAttribute('data-rotation', '90');
  const turned = await saveAsDownload(page);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(turned.bytes.subarray(0, original.length).equals(original)).toBe(true);
  expect(turned.bytes.length).toBeGreaterThan(original.length);

  // Removing a page rewrites the file, which breaks the signature: the core asks.
  await tool(page, 'Delete').click();
  await expect(cards(page)).toHaveCount(0);
  await saveButton(page).click();
  const dialog = page.getByRole('dialog', { name: "Saving will break this document's signature" });
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  const [download] = await Promise.all([
    page.waitForEvent('download'),
    dialog.getByRole('button', { name: 'Save a copy' }).click(),
  ]);
  // The app names it "rsa2048_sha256 (copy).pdf"; WebKit on Linux reports the
  // suggested name with the space turned into "_", so accept either.
  expect(download.suggestedFilename()).toMatch(/^rsa2048_sha256[ _]\(copy\)\.pdf$/);
  await expect(
    page.getByText(
      'Downloaded a copy as rsa2048_sha256 (copy).pdf. The opened file was not changed and still has unsaved changes.',
    ),
  ).toBeVisible();
  const copy = readFileSync(await download.path());
  expect(copy.subarray(0, 5).toString('latin1')).toBe('%PDF-');
  // A rewrite, not an increment on the signed bytes.
  expect(copy.subarray(0, original.length).equals(original)).toBe(false);

  await openBytes(page, 'turned.pdf', turned.bytes);
  await openOrganize(page);
  await expect(cards(page).nth(0)).toHaveAttribute('data-rotation', '90');
});
