// Comments and annotation navigation: Next/Previous bring an annotation that
// is off screen into view (on a turned page too) and select it when it can be
// edited; the panel lists the notes the file carries with the session's own,
// follows undo, and reads them. All of it works where annotating is forbidden.

import { expect, test, type Locator, type Page } from '@playwright/test';
import { buildPdf, type PageSpec } from './pdf';
import { expectOpened, loadApp, openBytes, pageSlot } from './support';

const LETTER: PageSpec = { widthPt: 612, heightPt: 792 };
const SCALE = 96 / 72;
/** `/P` granting everything but bit 6, adding or changing annotations. */
const ANNOTATE_PROHIBITED = -4 & ~(1 << 5);

const button = (page: Page, name: string) => page.getByRole('button', { name, exact: true });
const status = (page: Page) => page.locator('.annotation-nav__status');
const panel = (page: Page) => page.getByRole('complementary', { name: 'Comments' });
const rows = (page: Page) => panel(page).locator('.comment-row__go');
const ring = (page: Page) => page.locator('.annotation-ring');

/** Four pages: a file note at the foot of page 2, which is turned 90°, and one on page 4. */
const commented = (options?: Parameters<typeof buildPdf>[1]) =>
  buildPdf(
    [
      LETTER,
      {
        ...LETTER,
        rotate: 90,
        // On the page as drawn (792 × 612): 60 pt from the left, 540 from the top.
        notes: [{ rect: [540, 60, 560, 80], contents: 'Check this total', author: 'Ana', date: "D:20240305143000+01'00'" }],
      },
      LETTER,
      { ...LETTER, notes: [{ rect: [100, 60, 120, 80], contents: 'Last page note' }] },
    ],
    options,
  );

async function expectInside(locator: Locator, scroller: Locator) {
  await expect(async () => {
    const inner = (await locator.boundingBox())!;
    const outer = (await scroller.boundingBox())!;
    expect(inner.x).toBeGreaterThanOrEqual(outer.x);
    expect(inner.y).toBeGreaterThanOrEqual(outer.y);
    expect(inner.x + inner.width).toBeLessThanOrEqual(outer.x + outer.width);
    expect(inner.y + inner.height).toBeLessThanOrEqual(outer.y + outer.height);
  }).toPass({ timeout: 5_000 });
}
const pagesArea = (page: Page) => page.getByLabel('Pages', { exact: true });

/** A session note near the top of page 1, with Select back in hand. */
async function addNote(page: Page, contents: string) {
  await button(page, 'Note').click();
  const slot = (await pageSlot(page, 1).boundingBox())!;
  await page.mouse.click(slot.x + 300 * SCALE, slot.y + 60 * SCALE);
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('textbox').fill(contents);
  await dialog.getByRole('button', { name: 'Add note' }).click();
  await expect(page.locator('.annotation-layer [data-kind="note"]')).toHaveCount(1);
  await button(page, 'Select').click();
}

test('Next and Previous reveal annotations off screen, on a turned page too, and wrap', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'commented.pdf', commented());
  await addNote(page, 'Session note');

  await button(page, 'Next annotation').click();
  await expect(status(page)).toHaveText('Note on page 1, 1 of 3');
  await expect(button(page, 'Note on page 1')).toHaveAttribute('aria-pressed', 'true');
  await expect(ring(page)).toHaveCount(0);

  await button(page, 'Next annotation').click();
  await expect(status(page)).toHaveText('Comment on page 2, 2 of 3');
  await expect(page.getByRole('toolbar', { name: 'Annotation', exact: true })).toHaveCount(0);
  await expectInside(ring(page), pagesArea(page));
  const slot = (await pageSlot(page, 2).boundingBox())!;
  const mark = (await ring(page).boundingBox())!;
  expect(Math.abs((mark.x - slot.x) / SCALE - 60)).toBeLessThan(2);
  expect(Math.abs((mark.y - slot.y) / SCALE - 540)).toBeLessThan(2);

  // From the keyboard: the focus is still on the button.
  await expect(button(page, 'Next annotation')).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(status(page)).toHaveText('Comment on page 4, 3 of 3');
  await expectInside(ring(page), pagesArea(page));
  await page.keyboard.press('Enter');
  await expect(status(page)).toHaveText('Note on page 1, 1 of 3');
  await expectInside(button(page, 'Note on page 1'), pagesArea(page));
  await button(page, 'Previous annotation').click();
  await expect(status(page)).toHaveText('Comment on page 4, 3 of 3');
});

test('the panel lists file and session notes, goes to them, reads them and follows undo', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'commented.pdf', commented());
  await button(page, 'Comments').click();
  await expect(rows(page)).toHaveText([/Check this total.*Ana · 2024-03-05 14:30/, 'Last page note']);

  await addNote(page, 'Session note');
  await expect(rows(page)).toHaveText(['Session note', /Check this total/, 'Last page note']);
  await button(page, 'Undo').click();
  await expect(rows(page)).toHaveCount(2);
  await button(page, 'Redo').click();
  await expect(rows(page)).toHaveCount(3);

  await rows(page).nth(2).click();
  await expect(rows(page).nth(2)).toHaveAttribute('aria-current', 'true');
  await expectInside(ring(page), pagesArea(page));
  // A press on the page lets the mark go.
  const slot = (await ring(page).boundingBox())!;
  await page.mouse.click(slot.x + 100, slot.y - 60);
  await expect(ring(page)).toHaveCount(0);

  const read = panel(page).getByRole('button', { name: /^Read note on page 2/ });
  await read.click();
  const dialog = page.getByRole('dialog', { name: 'Note on page 2' });
  await expect(dialog.getByRole('textbox')).toHaveValue('Check this total');
  await expect(dialog.getByRole('textbox')).not.toBeEditable();
  await expect(dialog).toContainText('Ana · 2024-03-05 14:30');
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(read).toBeFocused();

  // The session's note is selected, and read from its toolbar.
  await rows(page).nth(0).click();
  const bar = page.getByRole('toolbar', { name: 'Annotation', exact: true });
  await bar.getByRole('button', { name: 'Read note…' }).click();
  const mine = page.getByRole('dialog', { name: 'Note on page 1' });
  await expect(mine.getByRole('textbox')).toHaveValue('Session note');
  await mine.getByRole('button', { name: 'Close' }).click();
  await expect(bar.getByRole('button', { name: 'Read note…' })).toBeFocused();

  // Escape in the panel closes it and goes back to its toggle.
  await rows(page).nth(0).focus();
  await page.keyboard.press('Escape');
  await expect(panel(page)).toHaveCount(0);
  await expect(button(page, 'Comments')).toBeFocused();
});

test('follows a page deleted in Organize, and its undo', async ({ page }) => {
  await loadApp(page);
  await openBytes(page, 'commented.pdf', commented());
  await button(page, 'Organize').click();
  await page.locator('.page-card').first().click();
  await page.getByRole('toolbar', { name: 'Organize' }).getByRole('button', { name: 'Delete', exact: true }).click();
  await expect(page.locator('.page-card')).toHaveCount(3);
  await button(page, 'View').click();
  await button(page, 'Comments').click();
  await expect(panel(page).getByRole('heading', { level: 3 })).toHaveText(['Page 1', 'Page 3']);
  await button(page, 'Undo').click();
  await expect(panel(page).getByRole('heading', { level: 3 })).toHaveText(['Page 2', 'Page 4']);
});

test('works where annotating is forbidden', async ({ page }) => {
  await loadApp(page);
  const bytes = commented({ encrypt: { permissions: ANNOTATE_PROHIBITED, userPassword: 'reader' } });
  await page.getByLabel('Open PDF').setInputFiles({ name: 'locked.pdf', mimeType: 'application/pdf', buffer: bytes });
  const password = page.getByRole('dialog', { name: 'Password required' });
  await password.getByLabel('Password').fill('reader');
  await password.getByLabel('Password').press('Enter');
  await expectOpened(page, 'locked.pdf');
  await expect(button(page, 'Note')).toHaveAttribute('aria-disabled', 'true');

  await button(page, 'Next annotation').click();
  await expect(status(page)).toHaveText('Comment on page 2, 1 of 2');
  await expectInside(ring(page), pagesArea(page));
  await button(page, 'Comments').click();
  await expect(rows(page).first()).toHaveAttribute('aria-current', 'true');
  await panel(page).getByRole('button', { name: /^Read note on page 2/ }).click();
  await expect(page.getByRole('dialog').getByRole('textbox')).toHaveValue('Check this total');
  await page.getByRole('dialog').getByRole('button', { name: 'Close' }).click();
  await expect(page.getByText('Unsaved changes.')).toHaveCount(0);
});

test('a narrow editor puts the panel under the pages, with no sideways overflow', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await loadApp(page);
  await openBytes(page, 'commented.pdf', commented());
  await button(page, 'Next annotation').click();
  await button(page, 'Comments').click();
  await expect(rows(page)).toHaveCount(2);
  const pages = (await pagesArea(page).boundingBox())!;
  const aside = (await panel(page).boundingBox())!;
  expect(aside.y).toBeGreaterThanOrEqual(pages.y + pages.height - 1);
  expect(pages.height).toBeGreaterThan(150);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  await rows(page).nth(1).click();
  await expectInside(ring(page), pagesArea(page));
});

for (const height of [844, 600]) {
  test(`a session note gone to at 390 × ${height} is clear of the annotation toolbar`, async ({ page }) => {
    await page.setViewportSize({ width: 390, height });
    await loadApp(page);
    await openBytes(page, 'plain.pdf', buildPdf([LETTER]));
    // As low as the only page goes: there must be room to scroll it over the toolbar.
    await pagesArea(page).evaluate((node) => (node.scrollTop = node.scrollHeight));
    await button(page, 'Note').click();
    const slot = (await pageSlot(page, 1).boundingBox())!;
    const area = (await pagesArea(page).boundingBox())!;
    const y = Math.min(slot.y + slot.height, area.y + area.height) - 4;
    await page.mouse.click(slot.x + slot.width / 2, y);
    const dialog = page.getByRole('dialog');
    await dialog.getByRole('textbox').fill('Low note');
    await dialog.getByRole('button', { name: 'Add note' }).click();
    await button(page, 'Select').click();
    await button(page, 'Comments').click();
    await rows(page).first().click();
    const note = button(page, 'Note on page 1');
    const bar = page.getByRole('toolbar', { name: 'Annotation', exact: true });
    await expect(note).toHaveAttribute('aria-pressed', 'true');
    await expectInside(note, pagesArea(page));
    await expect(async () => {
      const noteBox = (await note.boundingBox())!;
      const barBox = (await bar.boundingBox())!;
      expect(noteBox.y + noteBox.height).toBeLessThanOrEqual(barBox.y);
    }).toPass({ timeout: 5_000 });
    // Everything in the toolbar can still be reached.
    await bar.getByRole('button', { name: 'Delete' }).click();
    await expect(rows(page)).toHaveCount(0);
  });
}

test('saying where it is moves no page, however long it reads', async ({ page }) => {
  for (const width of [1280, 1000, 390]) {
    await page.setViewportSize({ width, height: 800 });
    await loadApp(page);
    const far = Array.from({ length: 120 }, (_, i) => ({
      ...LETTER,
      notes: i === 119 ? Array.from({ length: 110 }, () => ({ rect: [100, 60, 120, 80] as const, contents: 'x' })) : undefined,
    }));
    await openBytes(page, 'far.pdf', buildPdf(far));
    await expect(button(page, 'Previous annotation')).toHaveAttribute('aria-disabled', 'false');
    const before = await pagesArea(page).boundingBox();
    await button(page, 'Previous annotation').click();
    await expect(status(page)).toHaveText('Comment on page 120, 110 of 110');
    expect(await pagesArea(page).boundingBox()).toEqual(before);
    const bar = (await page.getByRole('toolbar', { name: 'Annotations' }).boundingBox())!;
    expect(bar.x + bar.width).toBeLessThanOrEqual(width);
  }
});
