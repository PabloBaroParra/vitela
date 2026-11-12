// Opening documents: the picker, a drop onto the window, and the password
// prompt of an encrypted PDF.

import { expect, test } from '@playwright/test';
import { dropFile, expectOpened, fixtureBytes, inkedPixelCount, loadApp, openFixture } from './support';

// tests/fixtures/README.md, encrypted/ corpus.
const ENCRYPTED = 'encrypted/aes_128_user_and_owner.pdf';
const USER_PASSWORD = 'user-aes-pass';

test('opens a PDF dropped onto the window', async ({ page }) => {
  await loadApp(page);
  await dropFile(page, 'dropped.pdf', fixtureBytes('forms/reportlab_acroform.pdf'));
  await expectOpened(page, 'dropped.pdf');
  expect((await inkedPixelCount(page)).inked).toBeGreaterThan(3_000);
});

test('a newly opened file replaces the open one', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, 'forms/reportlab_acroform.pdf');
  await openFixture(page, 'compress/vector_only.pdf');
  await expect(page.getByText('vector_only.pdf — 3 pages')).toBeVisible();
  await expect(page.getByText('reportlab_acroform.pdf —')).toHaveCount(0);
});

test('asks for the password of an encrypted PDF, refuses a wrong one, opens with the right one', async ({ page }) => {
  await loadApp(page);
  await page.getByLabel('Open PDF').setInputFiles(fixtureBytesFile(ENCRYPTED));

  const dialog = page.getByRole('dialog', { name: 'Password required' });
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  const password = dialog.getByLabel('Password');
  await expect(password).toBeFocused();

  await password.fill('not-the-password');
  await dialog.getByRole('button', { name: 'Open' }).click();
  await expect(dialog.getByRole('alert')).toHaveText('Wrong password. Try again.');
  await expect(dialog).toBeVisible();

  await password.fill(USER_PASSWORD);
  await password.press('Enter');
  await expect(dialog).toBeHidden();
  await expectOpened(page, 'aes_128_user_and_owner.pdf');
  expect((await inkedPixelCount(page)).inked).toBeGreaterThan(0);
});

test('Escape cancels the password prompt and keeps the open document', async ({ page }) => {
  await loadApp(page);
  await openFixture(page, 'forms/reportlab_acroform.pdf');
  await page.getByLabel('Open PDF').setInputFiles(fixtureBytesFile(ENCRYPTED));

  const dialog = page.getByRole('dialog', { name: 'Password required' });
  await expect(dialog).toBeVisible({ timeout: 30_000 });
  await page.keyboard.press('Escape');

  await expect(dialog).toBeHidden();
  await expect(page.getByText('reportlab_acroform.pdf — 1 page')).toBeVisible();
  await expect(page.locator('.page-slot[data-page="1"] canvas')).toHaveAttribute('data-painted', 'true');
});

function fixtureBytesFile(relativePath: string) {
  return {
    name: relativePath.split('/').pop()!,
    mimeType: 'application/pdf',
    buffer: fixtureBytes(relativePath),
  };
}
