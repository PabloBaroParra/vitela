# uniffi-bindgen-cs spike (Batch 1, T-006..T-010)

Validates `uniffi-bindgen-cs` (Rust -> C#) as the FFI bridge for the future
Windows shell (Batch 10) before any real `pdf-ffi` / WinUI3 work begins. See
`sdd/pdf-editor-mvp/design` ("uniffi-bindgen-cs spike (detailed plan)") and
`sdd/pdf-editor-mvp/spike-uniffi-cs-decision` for the recorded go/no-go call.

This crate is **excluded** from the root workspace (`Cargo.toml` ->
`[workspace] exclude`) so its pinned `uniffi` version can never collide with
the main workspace's dependency graph or break `cargo build --workspace`.

## Version pinning — the #1 gotcha

`uniffi-bindgen-cs` is versioned **separately** from `uniffi-rs` and only
works against the *exact* `uniffi-rs` version it was built against. The
project encodes this in its git tags: `vX.Y.Z+vA.B.C` means generator version
`X.Y.Z` targets `uniffi-rs` version `A.B.C`.

This spike uses:
- `uniffi-bindgen-cs` tag `v0.11.0+v0.31.0`
- `uniffi = "=0.31.0"` (exact pin) in `Cargo.toml`

Confirmed directly from the tool's own workspace manifest at that tag
(`uniffi = { version = "0.31.0", ... }`, `uniffi_bindgen = "0.31.0"`, etc.) —
do not assume the crates.io "latest" `uniffi` (0.32.0 at the time of this
spike) is compatible; it is not guaranteed to be, and pairing mismatched
versions produces confusing runtime `UniffiContractVersionException` /
`UniffiContractChecksumException` failures rather than a clean compile error.

If bumping either side, bump both together and re-verify against the
compatibility tags at
https://github.com/NordSecurity/uniffi-bindgen-cs/tags.

## Prerequisites

- Rust toolchain (this spike was validated on `rustc 1.97.0`)
- `uniffi-bindgen-cs`, installed pinned to the tag above:
  ```
  cargo install uniffi-bindgen-cs --git https://github.com/NordSecurity/uniffi-bindgen-cs --tag v0.11.0+v0.31.0 --locked
  ```
- .NET SDK 8.0+ (validated on `dotnet 9.0.315`, targeting `net9.0`)

## Layout

```
spikes/uniffi-cs/
├── Cargo.toml            # standalone crate, own [workspace], pinned uniffi = "=0.31.0"
├── src/lib.rs             # the dummy UniFFI interface + Rust-side unit tests
├── csharp-host/
│   ├── SpikeHost.csproj   # net9.0 console app
│   ├── Program.cs         # exercises every generated binding, real Stopwatch timings
│   └── generated/         # uniffi-bindgen-cs output (regenerated, not hand-edited)
└── spike.sh               # end-to-end: cargo test -> cargo build -> bindgen -> dotnet build -> run
```

## Running it

```bash
./spike.sh
```

Runs, in order: `cargo test --release` (Rust unit tests, strict-TDD evidence
for the in-process logic), `cargo build --release` (produces
`target/release/uniffi_cs_spike.dll`), `uniffi-bindgen-cs --library ...`
(regenerates `csharp-host/generated/uniffi_cs_spike.cs`), `dotnet build -c
Release`, then runs the host — which prints `[PASS]`/`[FAIL]` per check plus
the real benchmark numbers, and exits non-zero if anything failed.

## What's tested (mapped to tasks)

| Task | What | Where |
|---|---|---|
| T-006 | String echo, non-ASCII round trip | `echo_string` |
| T-006 | Small byte-array round trip | `bytes_round_trip` |
| T-007 | >=8MB byte-array round trip, 20 measured iterations + 3 warmup, Stopwatch timings (min/p50/avg/max) | `bytes_round_trip` called with an 8MB+37-byte buffer from `Program.cs` |
| T-008 | Error enum -> typed C# exception (not a string), both a unit variant and a variant carrying a field | `SpikeError::EmptyInput` / `SpikeError::DivideByZero { numerator }` -> generated `SpikeException.EmptyInput` / `SpikeException.DivideByZero` |
| T-009 | Async callback/event delivery, Rust background thread -> C#, call returns before delivery, events arrive in order | `SpikeEventListener` callback interface + `fire_events` |

## Key findings (see engram `spike-uniffi-cs-decision` for the full go/no-go writeup)

- All 5 spike checks passed on a real run (not simulated) — see decision doc
  for exact benchmark numbers.
- The generated exception hierarchy nests error variants as inner classes of
  a base exception type (e.g. `SpikeException.DivideByZero : SpikeException`),
  which C# `catch` blocks can discriminate directly by type — confirms the
  design doc's requirement that `FfiError` "mapped to ... C# exception ...
  no raw error strings" is achievable.
- Callback interfaces (`#[uniffi::export(callback_interface)]`) correctly
  deliver from a Rust-spawned background OS thread to the C# listener
  instance; the exported Rust function returns immediately (fire-and-forget),
  matching the async `PageRendered`-style notification contract the real
  `pdf-ffi` will need.
- The measured >=8MB round-trip cost came in noticeably higher than the
  design doc's "single-digit milliseconds" assumption for a raw memcpy — see
  the decision doc for numbers and the reason (marshaling involves allocation
  + copy on *each* side of *each* direction, not a single memcpy). This does
  not change the go/no-go verdict (still comfortably inside the 1.5s render
  budget) but is a design-doc footnote worth correcting.
