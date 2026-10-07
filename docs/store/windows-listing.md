# Microsoft Store listing — Vitela for Windows

The values to enter in Partner Center for the first, manual submission of
product `9N5CXSH1T5DM`. Later submissions are made by
[`windows-store.yml`](../../.github/workflows/windows-store.yml) from a release
tag, and only replace the package. Edit the listing in Partner Center.

The package declares a single language, **English (United States)**, because
the UI is English only. Partner Center therefore asks for an `en-us` listing
only.

## Pricing and availability

| Field | Value |
|---|---|
| Markets | All markets |
| Visibility | Public (see *Beta* below) |
| Pricing | Free |
| Free trial | No free trial |
| Release | As soon as it passes certification |

### Beta

The Store has no public beta channel. This first release is labelled beta in
its listing: in the *Description*, in *What's new* and through the `0.1` version.

## Properties

| Field | Value |
|---|---|
| Category | Productivity |
| Subcategory | — |
| Privacy policy URL | `https://github.com/PabloBaroParra/vitela/blob/main/PRIVACY.md` |
| Website | `https://github.com/PabloBaroParra/vitela` |
| Support contact info | `https://github.com/PabloBaroParra/vitela/issues` |
| Product declarations | Leave all unchecked |
| System requirements | Defaults (x64, Windows 10 1809 or later comes from the package) |

## Age ratings (IARC questionnaire)

| Question | Answer |
|---|---|
| Category | **Productivity / utility (not a game, not a web browser)** |
| Violence, fear, sexuality, language, drugs, gambling | No to every question |
| Does the app let users interact or exchange content? | **No** |
| Does the app share the user's location with others? | **No** |
| Does the app allow purchases of digital goods? | **No** |
| Does the app give unrestricted internet access? | **No** — Vitela makes no network connections |
| Does the app collect personal information? | **No** |

Expected result: **3+ / Everyone** in every rating system.

## Store listing (English, United States)

### Description

```
Vitela is a fast, private PDF editor that works entirely on your computer. No account, no cloud upload, no ads and no tracking: your documents never leave your device.

BETA — this is an early release. Expect rough edges, and please report anything that breaks on the project's issue tracker.

Read
• Open any PDF, including password-protected ones
• Smooth multi-page scrolling, fit-to-width and text search
• Print with the system print dialog

Annotate and edit
• Highlight, underline and strike out text
• Draw, add shapes, stamps and sticky notes
• Edit document metadata: title, author, subject and keywords

Organize
• Reorder, rotate and delete pages with drag and drop
• Combine several PDFs into one
• Extract pages to a new PDF or split a document into several files

Protect, sign and share
• Encrypt a PDF with a password
• Sign with a certificate from Windows, a .pfx file or a smart card
• Export pages as PNG or JPEG images
• Compress a PDF to make it smaller

Vitela is open source and built on a shared Rust core with a native WinUI interface.
```

### What's new in this version

```
First public beta of Vitela for Windows.
```

### Product features (one per line, up to 20)

```
Works offline — documents never leave your device
No account, no ads, no tracking
Open password-protected PDFs
Search text across the whole document
Highlight, underline and strike out text
Draw, add shapes, stamps and notes
Reorder, rotate and delete pages
Merge several PDFs into one
Extract or split pages into new files
Protect a PDF with a password
Sign with a Windows, .pfx or smart-card certificate
Export pages as PNG or JPEG
Compress PDFs to a smaller size
```

### Short description

```
A private PDF editor that works offline: read, annotate, organize, protect and sign PDFs without an account.
```

### Search terms (up to 7)

```
PDF
PDF editor
merge PDF
split PDF
sign PDF
compress PDF
PDF reader
```

### Copyright and trademark info

```
© 2026 Pablo Baro Parra. Licensed under MIT or Apache-2.0.
```

### Screenshots

At least one is required; up to ten are allowed. PNG, 1920×1080 recommended
(1366×768 minimum). The files are in
[`docs/store/screenshots/`](screenshots/), in the order they should appear. They show a fictional document, *Northwind
Annual Report*. Never capture the Home or Recent screens from a real machine:
those screens list the Windows Recent items, so they show the user's own files.

### Store logos

Optional, but without them the Store falls back to the package's own logo
assets. The files are in [`docs/store/windows/`](windows/):

| Partner Center slot  | File                       | Size      |
| -------------------- | -------------------------- | --------- |
| 1:1 Box art          | `box-art-2160x2160.png`    | 2160×2160 |
| 2:3 Poster art       | `poster-art-1440x2160.png` | 1440×2160 |
| 1:1 App tile icon    | `app-tile-icon-300.png`    | 300×300   |

## Packages

Upload `Vitela.Windows_<version>_x64.msix` from
`build/windows-store/packages/` (produced by
`scripts/package-windows-store.ps1 -Version 0.1.101.0` with the Partner Center
identity). Device family availability: **Windows 10/11 Desktop** only.

## Submission options

The package declares the restricted capability `runFullTrust`, the
standard one for a desktop app. Partner Center asks why. Answer:

```
Vitela is a WinUI 3 desktop (Win32) application packaged as MSIX. runFullTrust is required for every packaged desktop app to run as a full-trust process. The app only reads files the user opens and, when signing, the certificate the user chooses.
```

## Notes for certification

```
No account or test credentials are needed. The app works offline. Use "Open" (or the built-in sample document on the Home screen) to load a PDF; every feature can be tried on the sample.
```
