// Document properties (T-255): the six text keys of /Info in a dialog, as on
// Android (Windows and Linux show the same keys in a panel), recorded as one
// undoable edit through the real core and written into /Info by Save. No
// shell syncs an XMP stream, so none is checked. A document whose
// permissions withhold modification shows its properties read-only, the
// gate every native shell asks.

import { readFileSync } from 'node:fs';
import { expect, test, type Locator, type Page } from '@playwright/test';
import { buildPdf, MODIFY_PROHIBITED } from './pdf';
import { documentInfo } from './pdfObjects';
import { expectOpened, loadApp, openBytes, openTool, toolsButton } from './support';

const LETTER = { widthPt: 612, heightPt: 792 };
const bar = (page: Page) => page.locator('.workspace__bar');
const propertiesDialog = (page: Page) => page.getByRole('dialog', { name: 'Document properties' });
const saveButton = (page: Page) => bar(page).getByRole('button', { name: 'Save', exact: true });

async function openDialog(page: Page): Promise<Locator> {
  await openTool(page, 'Document properties');
  const dialog = propertiesDialog(page);
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  return dialog;
}

test('edits the title and author as one undoable edit, and Save writes them into /Info', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'report.pdf', buildPdf([LETTER]));

  let dialog = await openDialog(page);
  await expect(dialog.getByLabel('Title')).toBeFocused();
  await expect(dialog.getByLabel('Title')).toHaveValue('');
  await dialog.getByLabel('Title').fill('Quarterly report');
  await dialog.getByLabel('Author').fill('Ana Peña');
  await dialog.getByRole('button', { name: 'Apply' }).click();
  await expect(propertiesDialog(page)).toHaveCount(0);
  await expect(toolsButton(page)).toBeFocused();
  await expect(page.getByText('Document properties updated. Changes are pending save.')).toBeVisible();
  await expect(page.getByText('Unsaved changes.')).toBeVisible();

  // One step in the document's history: Undo takes both keys back, Redo returns them.
  const undo = page.getByRole('button', { name: 'Undo', exact: true });
  const redo = page.getByRole('button', { name: 'Redo', exact: true });
  await undo.click();
  await expect(redo).toBeEnabled();
  dialog = await openDialog(page);
  await expect(dialog.getByLabel('Title')).toHaveValue('');
  await expect(dialog.getByLabel('Author')).toHaveValue('');
  await page.keyboard.press('Escape');
  await expect(propertiesDialog(page)).toHaveCount(0);
  await redo.click();
  await expect(undo).toBeEnabled();
  dialog = await openDialog(page);
  await expect(dialog.getByLabel('Title')).toHaveValue('Quarterly report');
  await expect(dialog.getByLabel('Author')).toHaveValue('Ana Peña');
  await dialog.getByRole('button', { name: 'Cancel' }).click();

  const [download] = await Promise.all([page.waitForEvent('download'), saveButton(page).click()]);
  const bytes = readFileSync(await download.path());
  const info = documentInfo(bytes);
  expect(info.Title).toBe('Quarterly report');
  expect(info.Author).toBe('Ana Peña');
  expect(info.Subject).toBeUndefined();

  // Read back by the app itself, from the saved file.
  await openBytes(page, 'saved.pdf', bytes);
  dialog = await openDialog(page);
  await expect(dialog.getByLabel('Title')).toHaveValue('Quarterly report');
  await expect(dialog.getByLabel('Author')).toHaveValue('Ana Peña');
  // Nothing new: Apply records nothing and the document stays clean.
  await dialog.getByRole('button', { name: 'Apply' }).click();
  await expect(propertiesDialog(page)).toHaveCount(0);
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);
  await expect(undo).toBeDisabled();
});

test('at 1280 px the tools take no row of their own: one bar row, and the toolbars keep theirs', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'report.pdf', buildPdf([LETTER]));
  await expect(toolsButton(page)).toBeVisible();
  // Nothing in the viewer's View toolbar opens a tool.
  await expect(page.getByRole('toolbar', { name: 'View' }).getByRole('button', { name: 'Properties' })).toHaveCount(0);
  const tops = await bar(page).evaluate((node) =>
    [...node.children].filter((child) => child.getBoundingClientRect().width > 0).map((child) => child.getBoundingClientRect()),
  );
  expect(Math.max(...tops.map((box) => box.top))).toBeLessThan(Math.min(...tops.map((box) => box.bottom)));
  const top = async (name: string) => (await page.getByRole('toolbar', { name }).boundingBox())!.y;
  // View shares its row with Text, and Annotations still fits beside Draw and History.
  expect(await top('View')).toBe(await top('Text'));
  expect(await top('Annotations')).toBe(await top('Draw'));
});

test('a document whose permissions forbid modification shows its properties read-only', async ({ page }) => {
  const locked = buildPdf([LETTER], { encrypt: { permissions: MODIFY_PROHIBITED, userPassword: 'user-e2e' } });
  await loadApp(page);
  await page.getByLabel('Open PDF').setInputFiles({ name: 'locked.pdf', mimeType: 'application/pdf', buffer: locked });
  const prompt = page.getByRole('dialog', { name: 'Password required' });
  await expect(prompt).toBeVisible({ timeout: 30_000 });
  await prompt.getByLabel('Password').fill('user-e2e');
  await prompt.getByLabel('Password').press('Enter');
  await expectOpened(page, 'locked.pdf');

  const dialog = await openDialog(page);
  await expect(dialog.getByRole('alert')).toHaveText('This document does not permit metadata changes.');
  for (const label of ['Title', 'Author', 'Subject', 'Keywords', 'Creator', 'Producer']) {
    await expect(dialog.getByLabel(label)).not.toBeEditable();
  }
  await expect(dialog.getByRole('button', { name: 'Apply' })).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: 'Close' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(propertiesDialog(page)).toHaveCount(0);
  await expect(toolsButton(page)).toBeFocused();
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);
});

test('on a phone, Tools opens the properties, which fit the screen', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 600 });
  await loadApp(page);
  await openBytes(page, 'report.pdf', buildPdf([LETTER]));
  const more = toolsButton(page);
  await openTool(page, 'Document properties');
  const dialog = propertiesDialog(page);
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  const box = (await dialog.boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(390);
  await dialog.getByLabel('Title').fill('Pocket');
  await dialog.getByRole('button', { name: 'Apply' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(more).toBeFocused();
});
