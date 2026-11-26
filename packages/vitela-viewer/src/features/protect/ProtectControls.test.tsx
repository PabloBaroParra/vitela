import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { EngineError, type Engine, type ProtectionPasswords } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import type { DeliverRequest, Delivery } from '../save/delivery';
import { ProtectControls } from './ProtectControls';
import { useProtect } from './useProtect';

const document: OpenDocument = {
  name: 'a.pdf',
  handle: 7,
  generation: 1,
  info: {
    pageCount: 1,
    pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }],
    pageIds: [0],
    textExtractionAllowed: true,
    documentAssemblyAllowed: true,
    blocks: [],
    importRefusal: null,
    annotationEditingAllowed: true,
    formFillAllowed: true,
  },
};

beforeAll(() => {
  HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute('open');
  };
});
afterEach(cleanup);

const PROTECTED = new Uint8Array([37, 80, 68, 70]);

function recording() {
  const delivered: DeliverRequest[] = [];
  const delivery: Delivery = {
    prepare: async () => {
      throw new Error('a protected copy needs no destination prepared');
    },
    deliver: async (request) => {
      delivered.push(request);
      return { how: 'downloaded', name: request.name, fellBack: null };
    },
  };
  return { delivery, delivered };
}

interface EngineOptions {
  readonly refusal?: string | null;
  readonly signed?: boolean | Error;
  /** What the core answers the passwords with: the bytes, or a refusal. */
  readonly answer?: (passwords: ProtectionPasswords) => Uint8Array;
}

/** The core's own two password refusals, so the dialog can be seen to show them. */
function coreRule({ openPassword, permissionsPassword }: ProtectionPasswords): Uint8Array {
  if (!openPassword || !permissionsPassword) {
    throw new EngineError('InvalidPasswords', 'both open and permissions passwords are required');
  }
  if (openPassword === permissionsPassword) {
    throw new EngineError('InvalidPasswords', 'the open and permissions passwords must be different');
  }
  return PROTECTED;
}

function protectEngine({ refusal = null, signed = false, answer = coreRule }: EngineOptions = {}) {
  const asked: string[] = [];
  const runs: [number, ProtectionPasswords, boolean][] = [];
  const engine = fakeEngine({
    protectionRefusal: async (handle) => (asked.push(`refusal:${handle}`), refusal),
    protectionInvalidatesSignatures: async (handle) => {
      asked.push(`signed:${handle}`);
      if (signed instanceof Error) throw signed;
      return signed;
    },
    protectToBytes: async (handle, passwords, invalidate) => {
      runs.push([handle, passwords, invalidate]);
      return answer(passwords);
    },
  });
  return { engine, asked, runs };
}

function Harness({
  engine,
  delivery,
  settle,
  focusFallback,
}: {
  engine: Engine;
  delivery: Delivery;
  settle?: () => Promise<unknown>;
  focusFallback?: () => void;
}) {
  const session = useProtect(engine, document, delivery, settle);
  return <ProtectControls session={session} documentName={document.name} focusFallback={focusFallback} />;
}

const protectButton = () => screen.getByRole('button', { name: 'Protect' });
const dialog = () => screen.findByRole('dialog', { name: 'Protect with a password' });
const openField = (shown: HTMLElement) => within(shown).getByLabelText('Password to open the document') as HTMLInputElement;
const permissionsField = (shown: HTMLElement) => within(shown).getByLabelText('Permissions password') as HTMLInputElement;

function type(shown: HTMLElement, open: string, permissions: string) {
  fireEvent.change(openField(shown), { target: { value: open } });
  fireEvent.change(permissionsField(shown), { target: { value: permissions } });
}

describe('ProtectControls', () => {
  it('asks the gates, then downloads a protected copy under -protected with both passwords', async () => {
    const { engine, asked, runs } = protectEngine();
    const { delivery, delivered } = recording();
    let settled = 0;
    render(<Harness engine={engine} delivery={delivery} settle={async () => void settled++} />);
    fireEvent.click(protectButton());

    const shown = await dialog();
    expect(asked).toEqual(['refusal:7', 'signed:7']);
    expect(openField(shown)).toBe(window.document.activeElement);
    for (const field of [openField(shown), permissionsField(shown)]) {
      expect(field.type).toBe('password');
      expect(field.autocomplete).toBe('new-password');
    }
    expect(within(shown).getByText(/Use different passwords\./)).toBeTruthy();

    type(shown, 'open-pw', 'permissions-pw');
    fireEvent.click(within(shown).getByRole('button', { name: 'Protect' }));
    expect(
      await screen.findByText('Downloaded a-protected.pdf, protected with a password. The open document is not protected.'),
    ).toBeTruthy();
    expect(settled).toBe(1);
    expect(runs).toEqual([[7, { openPassword: 'open-pw', permissionsPassword: 'permissions-pw' }, false]]);
    expect(delivered).toEqual([{ bytes: PROTECTED, name: 'a-protected.pdf', destination: { kind: 'newFile' } }]);
    expect(screen.queryByRole('dialog')).toBeNull();
    // The passwords went with the dialog: reopened, its fields are empty.
    fireEvent.click(protectButton());
    const again = await dialog();
    expect(openField(again).value).toBe('');
    expect(permissionsField(again).value).toBe('');
  });

  it("keeps the dialog open with the core's sentence for a missing or repeated password, and nothing is delivered", async () => {
    const { engine, runs } = protectEngine();
    const { delivery, delivered } = recording();
    render(<Harness engine={engine} delivery={delivery} />);
    fireEvent.click(protectButton());
    const shown = await dialog();

    type(shown, 'open-pw', '');
    fireEvent.click(within(shown).getByRole('button', { name: 'Protect' }));
    expect(await within(shown).findByText('Both open and permissions passwords are required.')).toBeTruthy();

    type(shown, 'same-pw', 'same-pw');
    fireEvent.click(within(shown).getByRole('button', { name: 'Protect' }));
    expect(await within(shown).findByText('The open and permissions passwords must be different.')).toBeTruthy();
    // What was typed stays, so it can be corrected.
    expect(openField(shown).value).toBe('same-pw');
    expect(runs).toHaveLength(2);
    expect(delivered).toEqual([]);
    expect(screen.getByRole('dialog')).toBeTruthy();
  });

  it('warns about a signed document and "Protect anyway" is the consent', async () => {
    const { engine, runs } = protectEngine({ signed: true });
    const { delivery, delivered } = recording();
    render(<Harness engine={engine} delivery={delivery} />);
    fireEvent.click(protectButton());
    const shown = await dialog();
    expect(within(shown).getByText(/This document is signed\./)).toBeTruthy();
    expect(within(shown).queryByRole('button', { name: 'Protect' })).toBeNull();

    type(shown, 'o', 'p');
    fireEvent.click(within(shown).getByRole('button', { name: 'Protect anyway' }));
    await waitFor(() => expect(delivered).toHaveLength(1));
    expect(runs[0]![2]).toBe(true);
  });

  it('says why a document cannot be protected and offers only Cancel', async () => {
    const { engine, asked, runs } = protectEngine({ refusal: 'this document does not permit protection changes' });
    render(<Harness engine={engine} delivery={recording().delivery} />);
    fireEvent.click(protectButton());
    const shown = await dialog();
    expect(within(shown).getByRole('alert').textContent).toBe('This document does not permit protection changes.');
    expect(asked).toEqual(['refusal:7']);
    expect(within(shown).getByRole('button', { name: 'Protect' })).toHaveProperty('disabled', true);
    expect(openField(shown).disabled).toBe(true);
    expect(within(shown).getByRole('button', { name: 'Cancel' })).toBe(window.document.activeElement);
    fireEvent.click(within(shown).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(runs).toEqual([]);
  });

  it('refuses rather than guessing "unsigned" when the signature question fails', async () => {
    const { engine } = protectEngine({ signed: new EngineError('SaveFailed', 'could not read the signatures') });
    render(<Harness engine={engine} delivery={recording().delivery} />);
    fireEvent.click(protectButton());
    const shown = await dialog();
    expect(within(shown).getByRole('alert').textContent).toBe('Could not read the signatures.');
    expect(within(shown).getByRole('button', { name: 'Protect' })).toHaveProperty('disabled', true);
  });

  it('closes the dialog and reports a failed save on the status line', async () => {
    const { engine } = protectEngine({
      answer: () => {
        throw new EngineError('SaveFailed', 'the writer broke');
      },
    });
    const { delivery, delivered } = recording();
    render(<Harness engine={engine} delivery={delivery} />);
    fireEvent.click(protectButton());
    const shown = await dialog();
    type(shown, 'o', 'p');
    fireEvent.click(within(shown).getByRole('button', { name: 'Protect' }));
    expect(await screen.findByText('Could not protect a.pdf: the writer broke')).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(delivered).toEqual([]);
  });

  it('gives the focus back to Protect, or to the fallback when Protect is not shown', async () => {
    const { engine } = protectEngine();
    let fallback = 0;
    render(<Harness engine={engine} delivery={recording().delivery} focusFallback={() => void fallback++} />);
    fireEvent.click(protectButton());
    fireEvent.click(within(await dialog()).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(protectButton()).toBe(window.document.activeElement);

    Object.defineProperty(protectButton(), 'checkVisibility', { value: () => false });
    fireEvent.click(protectButton());
    fireEvent.click(within(await dialog()).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(fallback).toBe(1);
  });
});
