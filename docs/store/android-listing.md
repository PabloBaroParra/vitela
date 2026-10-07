# Google Play listing — Vitela for Android

The values to enter in Play Console for app `dev.vitela.pdf`. Later releases
are uploaded by
[`android-release.yml`](../../.github/workflows/android-release.yml) from a
release tag, and only replace the bundle. Edit the listing in Play Console.

The UI is English only, so the default language is **English (United
States) – en-US**, and that is the only listing.

## Testing first

New personal developer accounts must run a **closed test** (track *Closed
testing - Alpha*) with at least 12 testers who opted in, for at least 14 days,
before they can apply for production. Beta and rc tags ship to that track
(`release-version.sh <tag> play-track` → `alpha`).

## App content (Policy → App content)

| Section | Answer |
|---|---|
| Privacy policy | `https://github.com/PabloBaroParra/vitela/blob/main/PRIVACY.md` |
| Ads | **No**, the app contains no ads |
| App access | **All functionality is available without special access**: no account, no login |
| Content ratings | IARC questionnaire, see below |
| Target audience | **13–15, 16–17 and 18+**. Leave the under-13 groups unticked. Ticking them puts the app under the Families policy, which a general-purpose PDF editor does not need |
| News app | No |
| COVID-19 contact tracing and status apps | No |
| Data safety | See below |
| Government apps | No |
| Financial features | None |
| Health apps | None |

### Content rating (IARC)

| Question | Answer |
|---|---|
| Category | **All other app types** (a utility, not a game, social or communication app) |
| Violence, fear, sexuality, language, controlled substances, gambling | No to every question |
| Does the app let users interact or exchange content? | **No** |
| Does the app share the user's location? | **No** |
| Does the app allow purchases of digital goods? | **No** |
| Does the app give unrestricted internet access? | **No**. The app has no internet permission |

Expected result: **Everyone / PEGI 3** in every rating system.

### Data safety

The answers follow from the code: the manifest declares **no permissions**
(not even `INTERNET`), and the only thing the app writes to its own storage is
a drawn signature, if the user chooses to remember it. That file never
leaves the phone. It is excluded from cloud backup and device transfer in
`res/xml/data_extraction_rules.xml`, and data that never leaves the device
is not "collected" in Play's terms.

| Question | Answer |
|---|---|
| Does your app collect or share any of the required user data types? | **No** |
| Is all of the user data collected by your app encrypted in transit? | Not asked once the answer above is No |
| Do you provide a way for users to request that their data is deleted? | Not asked once the answer above is No |

Resulting label: **No data collected · No data shared with third parties**.

## Store settings

| Field | Value |
|---|---|
| App or game | App |
| Category | **Productivity** |
| Tags | PDF, Document editor, Productivity tools (pick the closest Play offers) |
| Email address | A public contact address. It is shown on the listing |
| Website | `https://github.com/PabloBaroParra/vitela` |
| Phone | Leave empty |

## Main store listing (English, United States)

### App name (max 30)

```
Vitela
```

### Short description (max 80)

```
Private, offline PDF editor. Edit, fill, sign, organize and compress PDFs.
```

### Full description (max 4000)

```
Vitela is a fast, private PDF editor that works entirely on your phone. No account, no cloud upload, no ads and no tracking: the app has no internet permission, so your documents never leave your device.

BETA — this is an early release. Expect rough edges, and please report anything that breaks on the project's issue tracker.

Read
• Open PDFs from your files, from "Open with", from the share sheet or by dragging them in from another app
• Password-protected PDFs
• Pinch to zoom, text search and text selection
• Print with the system print dialog

Annotate and edit
• Highlight, underline and strike out text
• Draw freehand and add shapes
• Move, resize and delete annotations, and step through them
• Retype and move text, and insert new text
• Insert, move, resize, replace and delete images
• Edit document metadata: title, author, subject and keywords

Forms
• Fill in existing form fields
• Add new form fields to a document

Organize
• Reorder, rotate and delete pages
• Combine several PDFs into one
• Split a document into several files

Protect, sign and share
• Encrypt a PDF with a password
• Draw a signature with your finger and, if you want, remember it on this phone only
• Sign with a certificate from a .pfx file
• Export pages as images
• Compress a PDF to make it smaller

Vitela is open source and built on a shared Rust core with a native Jetpack Compose interface. It is also available for Windows and Linux.
```

### Graphics

| Asset | Requirement | Status |
|---|---|---|
| App icon | 512 × 512 PNG, 32-bit, up to 1 MB | To make from the launcher icon |
| Feature graphic | 1024 × 500 PNG or JPEG, no transparency | To make |
| Phone screenshots | 2–8, PNG or JPEG, 16:9 or 9:16, each side 320–3840 px | To capture |
| Tablet screenshots (7" and 10") | Optional, but needed to be featured on tablets | Optional |

As on Windows, screenshots must show a fictional document. Never capture a
screen that lists the tester's real files.
