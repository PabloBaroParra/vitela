// Forms: fill the four kinds of field of the repo's AcroForm from the panel
// (a field is filled in when the focus leaves it), see the page painted with
// the value, save and reopen to read the values back; fill a signed form,
// which saves as an increment on its signed bytes; and a document that does
// not allow filling in shows its fields read-only.

import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { openForms, pagePixels, signedForm } from './forms';
import { buildPdf } from './pdf';
import { expectOpened, expectPagePainted, loadApp, openBytes, openFixture } from './support';

const ACROFORM = 'forms/reportlab_acroform.pdf';
/** `/P` granting everything but bit 6, which annotating and filling in need. */
const ANNOTATE_PROHIBITED = -4 & ~(1 << 5);

const saveButton = (page: Page) => page.getByRole('button', { name: 'Save', exact: true });

async function saveAsDownload(page: Page): Promise<Buffer> {
  const [download] = await Promise.all([page.waitForEvent('download'), saveButton(page).click()]);
  return readFileSync(await download.path());
}

test('fills every kind of field, paints the page with it, and saves values that reopen', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, ACROFORM);
  const panel = await openForms(page);
  const name = panel.getByLabel('full_name');
  await expect(name).toHaveValue('Ada Lovelace');

  // Focusing a field rings it on the page.
  await name.focus();
  await expect(page.locator('.page-slot[data-page="1"] .annotation-ring')).toHaveCount(1);

  const before = await pagePixels(page);
  await name.fill('Grace Hopper');
  // Nothing is filled in while typing...
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);
  // ...Tab leaves the field (and goes on to the next one): now it is.
  await name.press('Tab');
  await expect(panel.getByLabel('notes')).toBeFocused();
  await expect(page.getByText('Unsaved changes.')).toBeVisible();
  await expectPagePainted(page, 1);
  await expect.poll(() => pagePixels(page), { timeout: 30_000 }).not.toBe(before);

  await panel.getByLabel('notes').fill('Line one\nLine two');
  await panel.getByLabel('subscribe').uncheck();
  await panel.getByRole('radio', { name: 'basic' }).check();
  await panel.getByLabel('country').selectOption('Chile');
  await expect(panel.getByLabel('subscribe')).not.toBeChecked();

  const bytes = await saveAsDownload(page);
  await openBytes(page, 'filled.pdf', bytes);
  const reopened = await openForms(page);
  await expect(reopened.getByLabel('full_name')).toHaveValue('Grace Hopper');
  await expect(reopened.getByLabel('notes')).toHaveValue('Line one\nLine two');
  await expect(reopened.getByLabel('subscribe')).not.toBeChecked();
  await expect(reopened.getByRole('radio', { name: 'basic' })).toBeChecked();
  await expect(reopened.getByLabel('country')).toHaveValue('Chile');
});

test('Undo takes a fill back, in the panel and on the page', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, ACROFORM);
  const panel = await openForms(page);
  const before = await pagePixels(page);
  await panel.getByLabel('full_name').fill('Grace Hopper');
  await panel.getByLabel('full_name').press('Enter');
  await expect.poll(() => pagePixels(page), { timeout: 30_000 }).not.toBe(before);

  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(panel.getByLabel('full_name')).toHaveValue('Ada Lovelace');
  await expect.poll(() => pagePixels(page), { timeout: 30_000 }).toBe(before);
});

test('a signed form saves the fill as an increment, keeping every signed byte', async ({ page }) => {
  const { original, bytes: opened } = signedForm();
  await loadApp(page);
  await openBytes(page, 'signed-form.pdf', opened);
  const panel = await openForms(page);
  await expect(panel.getByLabel('signer_note')).toHaveValue('Pending');

  await panel.getByLabel('signer_note').fill('Approved');
  await panel.getByLabel('signer_note').press('Enter');
  await expect(page.getByText('Unsaved changes.')).toBeVisible();

  const saved = await saveAsDownload(page);
  // No signature question: nothing signed is rewritten.
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(saved.subarray(0, opened.length).equals(opened)).toBe(true);
  expect(saved.subarray(0, original.length).equals(original)).toBe(true);
  expect(saved.length).toBeGreaterThan(opened.length);
  // The form still lists the signature field next to the one filled in.
  expect(saved.subarray(opened.length).toString('latin1')).toMatch(/\/Fields\s*\[\s*9 0 R\s+11 0 R\s*\]/);

  await openBytes(page, 'signed-filled.pdf', saved);
  const reopened = await openForms(page);
  await expect(reopened.getByLabel('signer_note')).toHaveValue('Approved');
});

test('a document that does not allow filling in shows its fields read-only', async ({ page }) => {
  const bytes = buildPdf(
    [{ widthPt: 612, heightPt: 792, fields: [{ name: 'applicant', rect: [72, 600, 372, 630], value: 'Ana Pérez' }] }],
    { encrypt: { permissions: ANNOTATE_PROHIBITED, userPassword: 'reader' } },
  );
  await loadApp(page);
  await page.getByLabel('Open PDF').setInputFiles({ name: 'locked-form.pdf', mimeType: 'application/pdf', buffer: bytes });
  const password = page.getByRole('dialog', { name: 'Password required' });
  await password.getByLabel('Password').fill('reader');
  await password.getByLabel('Password').press('Enter');
  await expectOpened(page, 'locked-form.pdf');

  const panel = await openForms(page);
  await expect(panel.getByText('This document does not allow its form fields to be filled in.')).toBeVisible();
  const applicant = panel.getByLabel('applicant');
  await expect(applicant).toHaveValue('Ana Pérez');
  await expect(applicant).not.toBeEditable();
  await applicant.focus();
  await page.keyboard.type('x');
  await applicant.press('Tab');
  await expect(applicant).toHaveValue('Ana Pérez');
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);
});
