# Performance baseline — Linux GTK shell

Phase 0 of the UI improvement plan: measure before changing anything. The
GTK shell carries opt-in timing traces (`apps/linux-gtk/src/app/perf/`); this
page says how to switch them on, how to read them, and holds the baseline
they are meant to produce.

## Enabling the traces

Off by default. Set `VITELA_PERF` to `1` (`true`, `on` and `yes` also work)
in the environment the shell starts from:

```sh
VITELA_PERF=1 scripts/run-linux.sh 2> perf.log
```

The variable is read once, at the first measurement. Off, each trace point
costs one check of a cached flag: no clock is read and nothing is allocated.

## Line format

One line per measurement, on stderr:

```text
perf <name> <millis>ms [key=value ...]
```

- `<name>` — a span name from the table below.
- `<millis>` — elapsed wall-clock milliseconds, three decimals.
- `key=value` — optional fields, space-separated, in a fixed order per span.
  Values are integers, decimals (three places), `true`/`false`, or bare
  identifiers; never quoted, never containing spaces.

```sh
grep '^perf ' perf.log                              # every measurement
grep '^perf render\.page ' perf.log | awk '{print $3}'   # one span's timings
```

Lines from worker threads are written whole, so they never interleave
mid-line, but they are ordered by when each span *ended*.

## Spans

"First page" spans end when the first page bitmap for that document is set
on screen after the event that started them.

| Span | Measures | Fields |
| --- | --- | --- |
| `open.worker` | An open's turn starting → the background open returning. Time an open spends queued behind another is not included. | `pages` |
| `open.pdfium` | pdfium opening the document (open worker). | |
| `open.permissions` | The permissions probe (open worker). | |
| `open.lopdf` | The decrypting lopdf load for the editable model (open worker). | |
| `open.read` | Reading the file's bytes for the save backing (file opens only). | `bytes` |
| `open.model` | Building the editable model from the lopdf document. | `pages` |
| `open.geometry` | The batched page-geometry round-trip. | |
| `open.first_page` | An open's turn starting → first page visible. Not reported for a document opened into a background tab, nor for one that needed a password. | `page` |
| `pages.build` | Building the page widgets and navigation buttons (`build_pages`): on open, on a tab switch, on a rebuild after save. | `pages` |
| `session.install` | Installing a session: control updates plus the first viewport pass. | `pages` |
| `preview.save` | The in-memory preview save (refresh worker). | `bytes` |
| `preview.reopen` | pdfium reopening the preview bytes, with page geometry (refresh worker). | |
| `preview.worker` | A preview refresh being started → its worker returning. | `ok` |
| `preview.install` | Main thread: swapping the new handle in, or rebuilding when the layout changed. | `pages`, `in_place` |
| `preview.refresh` | A preview refresh being started → the new handle installed. | `pages`, `in_place` |
| `preview.first_page` | A preview refresh being started → first page re-rendered from the new handle. | `page` |
| `save.worker` | A write (save, sign, protect) being dispatched → its worker returning. | `ok` |
| `save.rebuild` | Main thread: installing the reopened document after the write. | `pages` |
| `save.total` | Write dispatched → reopened document installed. | `pages` |
| `layout.refresh` | One `refresh_layout` that actually refitted: a zoom step, a resize, a scale change. | `pages`, `zoom` (percent) |
| `zoom.first_page` | A refit → first page rendered at the new size. | `page` |
| `zoom.first_tiles` | A refit → first batch of deep-zoom tiles placed (only at zooms that use tiles). | `page` |
| `viewport.update` | One `update_viewport` (every scroll tick). **Only reported at 2 ms or more.** | `renders`, `tile_batches` (jobs dispatched) |
| `tab.switch` | Parking the active tab and putting the other on screen. | `pages` |
| `tab.first_page` | A tab switch → first page visible in the new tab. | `page` |
| `organize.show` | Showing an Organize view (entering the screen, or switching Documents/Pages), including building its cards. | `view` |
| `organize.grid` | A full rebuild of the Pages grid (`populate_grid`). | `pages` |
| `organize.thumbnails` | Showing an Organize view → the last thumbnail it dispatched landing. Not reported when every card came from the cache. | `count` |
| `search.worker` | A search being dispatched → pdfium's answer. | `matches`, `query_chars` |
| `search.results` | A search being dispatched → results applied to the window. | `matches`, `query_chars` |
| `render.page` | One page render as the shell sees it: dispatched → bitmap back on the main thread, queueing included. | `page`, `dpi`, `visible`, `ok` |
| `render.tiles` | One deep-zoom tile batch, likewise. | `page`, `dpi`, `tiles`, `ok` |

Notes for reading them:

- A refit also happens right after an open and after a tab switch, so those
  report `layout.refresh` and `zoom.first_page` alongside their own spans.
- A preview refresh requested while another is running is replayed when that
  one ends; its clock starts at the replay.
- `open.*` sub-steps are reported by everything that opens a document
  through the shell's open path (recovery restore, the reopen after a save),
  not only by a user's open.
- `render.page` with `ok=false` is usually a cancelled render (scrolled
  away, refitted), not a failure.

## Reference PDF

A text-heavy document — a heading and 44 lines of text on every page — so
search and content editing have work to do. It is generated, not committed;
the default output directory (`tests/fixtures/large/`) is git-ignored. The
same page count always yields the same bytes.

```sh
cargo run --release -p gen-fixtures --example perf_reference                 # 1600 pages
cargo run --release -p gen-fixtures --example perf_reference -- 400          # another page count
cargo run --release -p gen-fixtures --example perf_reference -- 400 out.pdf  # another destination
```

Default output: `tests/fixtures/large/perf_reference_<pages>pg.pdf`. Every
heading reads `Reference page N of M` (one match per query); body words such
as `vellum` match on nearly every page.

The image-heavy sibling for raster throughput is the existing
`tests/fixtures/large/perf_200pg.pdf` (see `tests/fixtures/README.md`).

## Baseline

Recorded 2026-11-27. Numbers are `median (min–max, n)` in milliseconds.

- Machine: WSL2 (Ubuntu) on Windows 11, 8 logical cores, WSLg Xwayland
  (`GDK_BACKEND=x11`), **no GPU — llvmpipe software rendering**. Treat the
  absolute numbers as pessimistic; compare runs on the same box only.
- Build: `cargo build --release -p linux-gtk`.
- Commit: `0861120` + uncommitted instrumentation
  (`feat/linux-perf-instrumentation`).
- Display scale / window size: default window, 1000x720, fit zoom 74 %.
- Input: driven with xdotool (XTest). File cache warm; "cold" below means a
  fresh process, not a cold disk.
- Reference: `perf_reference_1600pg.pdf` (1600 pages, 1 848 064 bytes).
  Contrast: `tests/fixtures/large/edit_reopen_10pg.pdf` (10 pages).

| Scenario | Span | 1600 pages | Notes |
| --- | --- | --- | --- |
| Open (fresh process, n=5) | `open.first_page` | 2512 (1941–2808) | 10 pages: 397 (367–417) |
| | `open.worker` | 1330 (1278–1651) | 10 pages: 187 (168–198) |
| | `open.pdfium` | 25.0 (23.9–28.9) | 10 pages: 19.1 |
| | `open.permissions` | 17.3 (14.0–89.0) | 10 pages: 3.4 |
| | `open.lopdf` | 13.4 (12.6–18.5) | 10 pages: 3.5 |
| | `open.read` | 7.5 (7.1–29.9) | 10 pages: 2.1 |
| | `open.model` | 1.3 (1.0–2.9) | 10 pages: 0.0 |
| | `open.geometry` | 1263 (1212–1478) | 10 pages: 12.9. **189 (186–242, n=3) for the same 1600 pages after the app re-saved them** — see findings |
| | `pages.build` | 781 (545–816) | 10 pages: 3.4 |
| | `session.install` | 5.6 (4.8–8.0) | 10 pages: 2.3 |
| | first `render.page` | 355 (55–360) | 10 pages: 181. Page 0 is rendered twice on open (24 dpi, then 52 dpi) |
| Rotate all pages in Organize (n=5) | `preview.refresh` | 1459 (1061–1487) | `in_place=false` |
| | `preview.first_page` | 1550 (1094–1582) | |
| | `preview.save` | 10.1 (6.9–24.7) | |
| | `preview.reopen` | 175 (161–183) | |
| | `preview.worker` | 484 (195–561) | two clusters: ~200 and ~480–560 |
| | `preview.install` | 898 (866–978) | of which `pages.build` 806 (767–857) |
| Save via chooser (n=4) | `save.total` | 1157 (1090–1263) | |
| | `save.worker` | 280 (255–286) | reopen inside it: `open.geometry` 179 |
| | `save.rebuild` | 874 (836–983) | of which `pages.build` 793 (759–867) |
| Zoom one step, Ctrl+wheel 50–600 % (n=48) | `layout.refresh` | 4.2 (3.3–6.8) | flat across zoom levels |
| | `zoom.first_page` | 30.8 (5.0–105.8) | worst at 400 % |
| | `zoom.first_tiles` | 38.8 (33.3–47.4, n=3) | fired only at 600 % |
| | `render.tiles` | 24.1 (17.5–32.1, n=6) | |
| Wheel scroll, 7 bursts of 150 ticks (~17 pages each) | `viewport.update` ≥ 2 ms | 12–21 per burst, worst 3.6 | median 2.6 (n=114) |
| | `render.page` | 11.0 (1.3–43.2, n=117) | all `visible=false` (rendered ahead), none cancelled |
| Switch tab to the 1600-page doc (n=5) | `tab.switch` | 854 (819–884) | of which `pages.build` 798 (785–849) |
| | `tab.first_page` | 944 (928–976) | |
| Switch tab to the 10-page doc (n=5) | `tab.switch` | 47.0 (34.4–57.6) | `pages.build` 5.9 |
| | `tab.first_page` | 64.6 (51.9–77.2) | |
| Enter Organize (Documents view, n=5) | `organize.show` | 38.7 (12.8–53.8) | |
| | `organize.thumbnails` | 155 (74–218) | `count=1` |
| Organize → Pages, first time (n=3) | `organize.grid` | 2364 (2316–2535) | |
| | `organize.show` | 2365 (2317–2536) | |
| | `organize.thumbnails` | 4589 (4573–4681) | `count=1600` |
| Organize → Pages again (n=3) | `organize.show` | 19.3 (18.7–19.8) | cards reused; back to Documents: 3.7 |
| Search `Reference` (1600 matches, n=3) | `search.worker` | 1727 (1717–1757) | |
| | `search.results` | 1729 (1721–1761) | |
| Search a body word (~22 000 matches, n=11) | `search.worker` | 1554 (1488–1797) | `vellum`, `meadow`, `harbor`, `quartz`, `ledger` |
| | `search.results` | 1558 (1490–1801) | |

### Findings

- `pages.build` costs ~0.5 ms per page (~800 ms at 1600 pages) and is paid
  on every open, tab switch, preview rebuild and save.
- The three parses on open (`open.pdfium` + `open.permissions` +
  `open.lopdf` + `open.read`) total ~63 ms; `open.geometry` and
  `pages.build` are what the first paint waits for.
- `open.geometry` depends on the file, not only the page count: 1263 ms on
  the generated reference, 189 ms on the app's own re-save of it
  (`open.first_page` 1514 ms, n=3). Part of the reference open cost is the
  fixture's structure.
- `layout.refresh` covers only the synchronous refit call. GTK's deferred
  allocation of 1600 page widgets is not inside any span.
- `open.worker` on the 10-page file is ~145 ms longer than its sub-steps
  add up to; on the 1600-page file the two agree.

### Not measured

- **Retype one text run** (`preview.*` with `in_place=true`): not
  attempted; the preview numbers above come from a whole-document rotate,
  which takes the rebuild path.
- **Scrolling the whole document**: wheel bursts covered about 50 pages;
  PageDown/End sent to the window did not scroll the canvas.
- **Rendering while a search runs**: Ctrl+wheel did not zoom while the Find
  entry had focus, and one page jump issued during a search completed no
  render until the search returned. Consistent with search blocking
  rendering, but a single probe.
- **Cold disk cache** and **a real GPU**.
