# Vitela — iOS (SwiftUI)

Experimental. The shell builds and its unit tests run on a simulator in CI; it
has never run on a physical iPhone, and signing, provisioning and distribution
are all out of scope until there is a paid Apple Developer account.

## Layout

Per the no-monolithic-shells rule in [CLAUDE.md](../../CLAUDE.md), the shell is
split by responsibility, and the model layer is shared with macOS rather than
duplicated:

```
apps/apple/Shared/        Shared with the macOS shell. Foundation/CoreGraphics
                          only — importing AppKit or UIKit here breaks the
                          other platform.
  PdfCore.swift           Value types, the PdfCoreClient protocol, ViewerFailure
  UniFfiPdfCoreClient.swift  The only source that calls the generated UniFFI API
  ViewerStore.swift       Page slots, zoom, render generations, apply/retry
  RenderedPage+CGImage.swift  RGBA buffer to CGImage
apps/apple/SharedTests/   Store tests, compiled into both platforms' test targets
apps/ios/Vitela/
  VitelaApp.swift         Entry point only
  ViewerViewModel.swift   Document picking (fileImporter) and render queueing
  Views/                  ViewerRootView, PageView, RenderedPage+Image (UIKit)
apps/ios/VitelaTests/     iOS-only view-model tests
apps/ios/Tests/           Portable shell tests for the deployment-floor gate
```

## Building

`--prepare` cross-compiles the FFI for the requested platform, rewrites its
install name to `@rpath`, and regenerates the Swift bindings:

```bash
bash scripts/build-ios.sh --prepare iphonesimulator
```

Then build and test with Xcode. CI selects the simulator by UDID because the
device names in the runner image change between releases:

```bash
xcodebuild -project apps/ios/VitelaIOS.xcodeproj -scheme VitelaIOS -destination 'platform=iOS Simulator,name=iPhone 16' test
```

PDFium comes from the pinned `bblanchon/pdfium-binaries` release. Device and
simulator are **different binaries**; they live in
`core/pdf-render/vendor/pdfium-iphoneos/` and
`core/pdf-render/vendor/pdfium-iphonesimulator/`, or are pointed at explicitly
with `PDFIUM_DYLIB`.

## Deployment floor

iOS 15.0, enforced in two independent ways:

- `--validate-deployment-floor` checks that the Xcode project's
  `IPHONEOS_DEPLOYMENT_TARGET` and the CI workflow agree on 15.0. This is a
  check on *declarations*.
- `--verify-deployment <platform>` reads the actual `minos` load command out of
  both dylibs the app loads at runtime. This is a check on *facts*, and it is
  the one that would catch a PDFium build that quietly requires a newer iOS.
