# Windows verification checkpoint — 2026-10-05

All 22 existing native smoke targets and seven normal-app UI Automation scenarios
passed locally on Windows x64. This is a development verification checkpoint,
not a release, CI result or full physical-device sign-off.

Baseline: `785c6874befdae42d99d6eec089e9d7f5c236883` plus the checkpoint's
signing-dialog, signing-source regression and window-lifecycle changes.

## Native runtime evidence

Each target below built with Visual Studio MSBuild in Debug/x64, ran with the
repository PDFium DLL and exited 0. Each latest assertion log was read and
contained PASS, with no FAIL or UNHANDLED entry.

| Area | Passing targets |
| --- | --- |
| Signing | SigningSmoke, SigningDialogsSmoke |
| Content | CanvasInsertSmoke, TextMoveSmoke, ContentDeleteSmoke, SelectionSmoke |
| Forms and colors | FormFillSmoke, FormToolbarSmoke, ColorDialogsSmoke |
| Organization | ImportPdfsSmoke, OrganizeDocumentsSmoke, OrganizePagesSmoke |
| Output and lifecycle | LifecycleDialogsSmoke, OutputDialogsSmoke, PrintSmoke, DocumentStatesSmoke |
| Navigation and search | PageNavigationSmoke, SearchSmoke |
| Annotations | AnnotationPositionSmoke, AnnotationSizeSmoke, NotePlacementSmoke, StampPlacementSmoke |

Builds used `Tests/<target>.targets` via `CustomAfterMicrosoftCommonTargets`,
isolated output directories under `C:/Users/nexty/AppData/Local/Temp/opencode/`
and separate `obj/x64/Debug/Final<target>/` intermediates. Native logs are in that
temporary directory. Temporary batch runners are local tooling, not repository
dependencies.

The navigation run first exposed an obsolete null-argument event invocation and
then missing XamlRoot initialization. The shell now attaches its root on Loaded
or activation, once, and removes subscriptions on close. Navigation, lifecycle
and organization-page smokes were rerun after this correction; the other native
runs preceded it. The final normal build and UI Automation runs include the fix.

## Final normal-app checks

| Command | Outcome |
| --- | --- |
| `dotnet run --project apps/windows/Pdf.Windows.Facade.Tests/Pdf.Windows.Facade.Tests.csproj` | Exit 0; all listed checks PASS |
| Visual Studio MSBuild `apps/windows/Pdf.Windows/Pdf.Windows.csproj -restore -p:Configuration=Debug -p:Platform=x64` with isolated output/intermediates | Exit 0; no warnings or errors reported |
| `powershell -NoProfile -File apps/windows/test-home.ps1 -TestSample` | Exit 0; PASS on each fresh UIA instance |
| `powershell -NoProfile -File apps/windows/test-editor-chrome.ps1 -TestComputerSigning` | Exit 0; PASS including real personal-store identity cancellation |
| `powershell -NoProfile -File apps/windows/test-print.ps1` | Exit 0; PASS native preview/cancellation twice; no job submitted |
| `powershell -NoProfile -File apps/windows/test-compress.ps1` | Exit 0; PASS |
| `powershell -NoProfile -File apps/windows/test-export-images.ps1` | Exit 0; PASS |
| `powershell -NoProfile -File apps/windows/test-lifecycle-dialogs.ps1` | Exit 0; PASS |
| `powershell -NoProfile -File apps/windows/test-organize.ps1` | Exit 0; PASS |
| `powershell -NoProfile -File apps/windows/test-metadata.ps1` | Exit 0; PASS |
| `python scripts/check_maintainability.py` | Exit 0; 128 advisory warnings across 652 maintained source files |
| `python scripts/check_readme_tables.py` | Exit 0; README status tables OK |
| `git diff --check` | Exit 0; LF/CRLF notice only |

`-TestComputerSigning` requires a usable certificate in CurrentUser/My. It
discovers identities and cancels; it does not sign with a user's private key.
Signing success tests use ephemeral RSA/ECDSA keys. A mismatched algorithm checks
the real cryptographic error mapping, not an actual provider-prompt cancellation.

Maintainability review prompt: PageNavigationSmoke remains a cohesive end-to-end
navigation/history scenario despite its decision-count warning. This correction
changes event setup and log isolation only; reassess scenario extraction when
adding further navigation behaviors.

## Remaining evidence and handoff

- Physical card/token, vendor-module/PIN success and provider authentication
  prompts: explicitly deferred by the maintainer, who has no device available.
- Physical pointer gestures/drop/autoscroll, theme/DPI transitions and actual
  printer output still need manual checks; native command-path tests do not
  substitute for them.
- No new CI, signed-package/MSIX validation, external signature-validator run
  or other-platform shell gate was performed locally for this diff.
- Android can use this verified Windows baseline as a comparison target;
  Android runtime readiness has not been assessed by this checkpoint.

## Publication gates — 2026-10-05

After the maintainer requested publication, the complete authored diff received
local Code Review and the following additional Windows-host gates passed:

| Command | Outcome |
| --- | --- |
| `cargo fmt --all -- --check` | Exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Exit 0 |
| `cargo build --workspace --locked` | Exit 0 |
| `cargo test --workspace --locked` | Exit 0; all executed groups passed, with existing opt-in performance/external-validator/network-namespace tests ignored by their declarations |

The Rust build/test used the repository PDFium DLL through
`PDFIUM_DYNAMIC_LIB_PATH`. These commands do not type-check the Linux or Apple
shells on a Windows host. No Bash script changed, so shellcheck is not applicable;
the modified PowerShell smoke was executed successfully.
