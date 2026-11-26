// Protect (T-251): the dialog, the real core's protected save, and the
// protected copy as a download — parity with the native Protect commands
// (two passwords that must differ, AES-128 with every permission granted,
// the signature warning). The web hands over a copy instead of reopening it
// in place, so the copy is reopened here through the app's own password
// prompt, once with each password.

import { readFileSync } from 'node:fs';
import { expect, test, type Download, type Locator, type Page } from '@playwright/test';
import { cards, openOrganize, tool } from './organize';
import { expectOpened, fixtureBytes, inkedPixelCount, loadApp, openFixture } from './support';

const THREE_PAGES = 'compress/vector_only.pdf';
const SIGNED = 'signed/rsa2048_sha256.pdf';
// tests/fixtures/README.md, encrypted/ corpus: its /P withholds modification.
const ENCRYPTED = 'encrypted/aes_128_user_and_owner.pdf';
const ENCRYPTED_USER_PASSWORD = 'user-aes-pass';
const OPEN_PASSWORD = 'open-e2e-pass';
const PERMISSIONS_PASSWORD = 'permissions-e2e-pass';

/** The workspace bar's Protect button (the dialog has one of its own). */
const protectButton = (page: Page) => page.locator('.workspace__bar').getByRole('button', { name: 'Protect', exact: true });
const protectDialog = (page: Page) => page.getByRole('dialog', { name: 'Protect with a password' });
const passwordPrompt = (page: Page) => page.getByRole('dialog', { name: 'Password required' });

async function openDialog(page: Page): Promise<Locator> {
  await protectButton(page).click();
  const dialog = protectDialog(page);
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  return dialog;
}

async function typePasswords(dialog: Locator, open: string, permissions: string): Promise<void> {
  await dialog.getByLabel('Password to open the document').fill(open);
  await dialog.getByLabel('Permissions password').fill(permissions);
}

/** Clicks `confirm` in the dialog and returns the download it starts. */
async function protectToDownload(page: Page, confirm: Locator): Promise<{ download: Download; bytes: Buffer }> {
  const [download] = await Promise.all([page.waitForEvent('download', { timeout: 60_000 }), confirm.click()]);
  return { download, bytes: readFileSync(await download.path()) };
}

/** Opens `bytes` through the picker, answering the password prompt with `password`. */
async function openWithPassword(page: Page, name: string, bytes: Buffer, password: string): Promise<void> {
  await page.getByLabel('Open PDF').setInputFiles({ name, mimeType: 'application/pdf', buffer: bytes });
  const prompt = passwordPrompt(page);
  await expect(prompt).toBeVisible({ timeout: 30_000 });
  await prompt.getByLabel('Password').fill(password);
  await prompt.getByLabel('Password').press('Enter');
  await expect(prompt).toBeHidden();
  await expectOpened(page, name);
}

test('protects a copy with the pending edits; it reopens with each password, every permission granted', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, THREE_PAGES);
  await openOrganize(page);
  await cards(page).nth(0).click();
  await tool(page, 'Delete').click();
  await expect(cards(page)).toHaveCount(2);

  const dialog = await openDialog(page);
  for (const label of ['Password to open the document', 'Permissions password']) {
    const field = dialog.getByLabel(label);
    await expect(field).toHaveAttribute('type', 'password');
    await expect(field).toHaveAttribute('autocomplete', 'new-password');
  }
  await expect(dialog.getByLabel('Password to open the document')).toBeFocused();
  await typePasswords(dialog, OPEN_PASSWORD, PERMISSIONS_PASSWORD);
  const { download, bytes } = await protectToDownload(page, dialog.getByRole('button', { name: 'Protect', exact: true }));
  expect(download.suggestedFilename()).toBe('vector_only-protected.pdf');
  expect(bytes.subarray(0, 5).toString('latin1')).toBe('%PDF-');
  expect(bytes.toString('latin1')).toContain('/Encrypt');
  await expect(protectDialog(page)).toHaveCount(0);
  await expect(protectButton(page)).toBeFocused();
  await expect(
    page.getByText('Downloaded vector_only-protected.pdf, protected with a password. The open document is not protected.'),
  ).toBeVisible();
  // Not an edit: the deletion is still unsaved and undoable.
  await expect(page.getByText('Unsaved changes.')).toBeVisible();
  await expect(tool(page, 'Undo')).toHaveAttribute('aria-disabled', 'false');

  // The copy needs a password: a wrong one is refused, the open one opens it.
  await page.getByRole('button', { name: 'View' }).click();
  await page.getByLabel('Open PDF').setInputFiles({ name: 'locked.pdf', mimeType: 'application/pdf', buffer: bytes });
  const prompt = passwordPrompt(page);
  await expect(prompt).toBeVisible({ timeout: 30_000 });
  await prompt.getByLabel('Password').fill('not-the-password');
  await prompt.getByRole('button', { name: 'Open' }).click();
  await expect(prompt.getByRole('alert')).toHaveText('Wrong password. Try again.');
  await prompt.getByLabel('Password').fill(OPEN_PASSWORD);
  await prompt.getByLabel('Password').press('Enter');
  await expectOpened(page, 'locked.pdf');
  await expect(page.getByText('locked.pdf — 2 pages')).toBeVisible();
  expect((await inkedPixelCount(page)).inked).toBeGreaterThan(0);
  // Every permission is granted under the open password: its pages can be reorganised.
  await openOrganize(page);
  await cards(page).nth(0).click();
  await expect(tool(page, 'Delete')).toHaveAttribute('aria-disabled', 'false');
  await page.getByRole('button', { name: 'View' }).click();

  // The permissions password opens it too.
  await openWithPassword(page, 'owner.pdf', bytes, PERMISSIONS_PASSWORD);
  await expect(page.getByText('owner.pdf — 2 pages')).toBeVisible();
});

test("refuses a missing or repeated password in the core's words, and nothing is written", async ({ page }) => {
  await loadApp(page);
  await openFixture(page, THREE_PAGES);
  let downloads = 0;
  page.on('download', () => downloads++);

  const dialog = await openDialog(page);
  await typePasswords(dialog, OPEN_PASSWORD, '');
  await dialog.getByRole('button', { name: 'Protect', exact: true }).click();
  await expect(dialog.getByRole('alert')).toHaveText('Both open and permissions passwords are required.');

  await typePasswords(dialog, OPEN_PASSWORD, OPEN_PASSWORD);
  await dialog.getByLabel('Permissions password').press('Enter');
  await expect(dialog.getByRole('alert')).toHaveText('The open and permissions passwords must be different.');
  await expect(dialog.getByLabel('Password to open the document')).toHaveValue(OPEN_PASSWORD);

  await page.keyboard.press('Escape');
  await expect(protectDialog(page)).toHaveCount(0);
  await expect(protectButton(page)).toBeFocused();
  // Reopened, the dialog has forgotten what was typed.
  const again = await openDialog(page);
  await expect(again.getByLabel('Password to open the document')).toHaveValue('');
  await expect(again.getByLabel('Permissions password')).toHaveValue('');
  await again.getByRole('button', { name: 'Cancel' }).click();
  expect(downloads).toBe(0);
});

test('a signed document asks first, and "Protect anyway" downloads a copy that opens', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, SIGNED);

  const dialog = await openDialog(page);
  await expect(dialog.getByText(/This document is signed\./)).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Protect', exact: true })).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(protectDialog(page)).toHaveCount(0);

  const again = await openDialog(page);
  await typePasswords(again, OPEN_PASSWORD, PERMISSIONS_PASSWORD);
  const { download, bytes } = await protectToDownload(page, again.getByRole('button', { name: 'Protect anyway' }));
  expect(download.suggestedFilename()).toBe('rsa2048_sha256-protected.pdf');
  await openWithPassword(page, 'signed-copy.pdf', bytes, OPEN_PASSWORD);
});

test('a document whose permissions withhold modification is refused in the dialog', async ({ page }) => {
  await loadApp(page);
  await openWithPassword(page, 'restricted.pdf', fixtureBytes(ENCRYPTED), ENCRYPTED_USER_PASSWORD);
  let downloads = 0;
  page.on('download', () => downloads++);

  const dialog = await openDialog(page);
  await expect(dialog.getByRole('alert')).toHaveText('This document does not permit protection changes.');
  await expect(dialog.getByRole('button', { name: 'Protect', exact: true })).toBeDisabled();
  await expect(dialog.getByLabel('Password to open the document')).toBeDisabled();
  await expect(dialog.getByRole('button', { name: 'Cancel' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(protectDialog(page)).toHaveCount(0);
  await expect(protectButton(page)).toBeFocused();
  expect(downloads).toBe(0);
});
