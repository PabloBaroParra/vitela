# @vitela/viewer

Vitela's PDF editor for any web page: view, organize (reorder, rotate,
delete, add pages from other PDFs), annotate, stamp, search, select text and
save, all in the browser on top of [`@vitela/core`](../vitela-core/README.md)
(the engine, as WebAssembly in a Web Worker). No server: documents never
leave the page.

React and React DOM 19.3.1 are peer dependencies. A page that does not use
React itself mounts the editor with `mountViewer` and never touches React,
but the two packages still have to be installed.

## Install

```bash
npm install @vitela/viewer react@19.3.1 react-dom@19.3.1
```

`@vitela/core` comes with it, at the same version. Both packages are published
from `npm-v*` tags of the Vitela repository
([`npm-release.yml`](../../.github/workflows/npm-release.yml)), with provenance.
To try a build that is not on npm, see
[`examples/embed-vite`](../../examples/embed-vite): it installs both packages
from `npm pack` tarballs.

## Usage

Import the stylesheet once, and **give the editor's container a height**:
the editor fills its container (`height: 100%`), it does not size itself.

### React

```tsx
import { VitelaViewer } from '@vitela/viewer';
import '@vitela/viewer/style.css';

export function Editor() {
  return (
    <div style={{ height: 600 }}>
      <VitelaViewer
        source="/files/report.pdf"
        onSave={async (bytes, name) => {
          await fetch(`/api/files/${encodeURIComponent(name)}`, { method: 'PUT', body: bytes });
        }}
      />
    </div>
  );
}
```

### Without React

```ts
import { mountViewer } from '@vitela/viewer';
import '@vitela/viewer/style.css';

const viewer = mountViewer(document.getElementById('editor')!, {
  source: '/files/report.pdf',
  onSave: (bytes, name) => upload(bytes, name),
});

viewer.open(someFile);      // a File, a Uint8Array or a URL
viewer.save();              // as the Save button: bytes go to onSave
viewer.destroy();           // unmounts, closes the document, stops the editor's engine
```

## Options

`<VitelaViewer>` props, and `mountViewer`'s options (the same, minus `ref`):

| Option | Default | What it does |
|---|---|---|
| `source` | none | The PDF to open once the engine is ready: a `File`, a `Uint8Array` or a URL (`string` or `URL`, fetched with `fetch`; relative URLs resolve against the page). A new value opens the new document. Bytes are copied: your array stays usable. |
| `name` | the URL's file name, else `document.pdf` | The document's name (status line, and the name `onSave` receives). A `File` keeps its own unless you give one. |
| `engine` | none | An engine you started with `createEngine()` from `@vitela/core`. It keeps running after the editor unmounts (the editor only closes its document). Passing a different engine closes the open document in the old one, drops its unsaved edits, and reopens the latest `source`/`open()` in the new one once it is ready. |
| `engineOptions` | `{}` | Options for the engine the editor starts itself when given no `engine` (see `createEngine` in `@vitela/core`: `assetBaseUrl`, `workerUrl`, `loadTimeoutMs`). Read once, at mount. That engine is terminated when the editor unmounts. |
| `onSave` | none | `(bytes, name) => void \| Promise<void>`: receives every save. A rejection fails the save and the editor says so; once it resolves the editor counts the edits as saved. Without it, a save writes over the opened file when the browser allows it (Chromium's file handles), else downloads. |
| `filePicker` | `true` | Whether the editor offers its "Open PDF" button. Dropping a PDF on the editor still opens it. |
| `initialScreen` | `'view'` | The screen every document opens in: `'view'` or `'organize'`. |
| `eventScope` | `'editor'` | Which page events the editor takes; see [Keys and drops](#keys-and-drops). `'window'` is for a page that is all editor. |
| `ref` | none | (React only) a `VitelaViewerHandle`: `open(source, name?)` and `save()`, as on `mountViewer`'s handle. |

`open` and `save` return nothing: the editor shows how the open or the save
went in its own status line, and saved bytes arrive in `onSave`. Calls on a
`mountViewer` handle after `destroy()` do nothing.

## Bundler (Vite)

Nothing in your `vite.config` is required by the editor. The consumer's
config can be just what your page needs anyway:

```ts
// vite.config.ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react'; // only if your page itself uses JSX

export default defineConfig({ plugins: [react()] });
```

What was checked, with Vite 8.3.8 and the packed tarballs
(`examples/embed-vite`, Playwright in Chromium, Firefox and WebKit, dev
server and production build):

- **Dev server: no `optimizeDeps.exclude` needed.** Vite pre-bundles
  `@vitela/viewer` together with `@vitela/core` into
  `node_modules/.vite/deps/`, and rewrites the engine's
  `new Worker(new URL('./worker/worker.js', import.meta.url))` to point back
  at `node_modules/@vitela/core/dist/worker/worker.js`, so the worker and its
  WASM load from the package. Adding
  `optimizeDeps: { exclude: ['@vitela/core', '@vitela/viewer'] }` also works,
  but changes nothing.
- **Build: no `worker` or `build` options needed.** The worker is bundled
  into one file (Vite's default worker format works) and both `.wasm` files
  and the pdfium glue are emitted as assets.

Other Vite versions and other bundlers were not tested. If yours does not
follow `new Worker(new URL('…', import.meta.url))`, serve the files yourself
and pass `engineOptions: { workerUrl, assetBaseUrl }` (see `@vitela/core`).

## Keys and drops

With the default `eventScope: 'editor'`:

- The editor's keys (Ctrl/⌘+Z and Ctrl/⌘+Y, Ctrl/⌘+C, Escape) act only when
  the focus is inside the editor, or, when nothing has the focus, when the
  user's last click, focus or drop was inside it. Keys elsewhere on your
  page are yours.
- Only files dropped **on the editor** open in it. A file dropped anywhere
  else on your page is your page's business: the editor does not cancel the
  event, so unless your page handles it, the browser does what it does with
  a dropped file (usually opens it in the tab).

## Styles: what is scoped and what is not

The editor's stylesheet stays under its root class `.vitela`: its tokens,
fonts and resets never reach your `html`, `body`, links or buttons. Two
things are **not** isolated (no Shadow DOM: it breaks the editor's dialogs
and text selection):

- **Your type selectors reach into the editor.** A rule such as
  `button { text-transform: uppercase }` or `p { margin: 2em }` applies to
  the editor's elements too; where both set the same property with the same
  specificity, the stylesheet loaded later wins. Prefer classes on your own
  elements, or scope your element rules (`.my-page button`).
- **The editor's class names have no prefix.** Its rules for `.dialog`,
  `.button--primary`, `.workspace`, `.page-card`, … are not under `.vitela`,
  so an element of your page with one of those classes picks them up.

The narrow-screen layout follows the viewport (`max-width: 600px`), not the
editor's container.

## Not supported yet

- Mounting the editor inside a Shadow DOM: its key and drop scoping relies on
  `contains`, which does not cross shadow roots.
- Two editors on the same page: each has its own root and scope, but this
  was not tested.
- Telling saves apart in `onSave`: when saving would break a signature, the
  editor asks, and both "Save anyway" and "Save a copy" hand the bytes to
  `onSave` under the document's name. A smaller copy from **Compress** also
  arrives in `onSave`, named `<name>-compressed.pdf`; it does not count the
  edits as saved (the open document is not the copy).
- Editing a page's existing text (it comes with the engine's content editing).

## Lower-level exports

`DocumentWorkspace` (the editor without the engine and source wiring:
`engine`, `intro`, `standIn`, …), `useEngineStatus` and `StatusLine` are the
pieces Vitela's own web app builds its shell from. They are public but
lower-level; a host page wants `VitelaViewer` or `mountViewer`.

## License

MIT OR Apache-2.0.
