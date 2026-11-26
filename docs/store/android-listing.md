# Google Play listing — Vitela for Android

The values to enter in Play Console for app `dev.vitela.pdf`. Later releases
are uploaded by
[`android-release.yml`](../../.github/workflows/android-release.yml) from a
release tag, and only replace the bundle. Edit the listing in Play Console.

The UI is in English and Spanish. The default language is **English (United
States) – en-US**; add a translation for **Spanish (Spain) – es-ES** with the
texts in [Spanish store listing](#spanish-store-listing-spanish-spain) below.
Release notes (`whatsnew`) are uploaded in en-US only, so Play shows them in
English to Spanish users too.

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
• Retype and move text, whole paragraphs included, and insert new text
• Make text bold, italic or underlined
• Insert, move, resize, replace and delete images
• Add a text or picture watermark
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
• Share the open PDF, unsaved edits included, with any other app

Available in English and Spanish.

Vitela is open source and built on a shared Rust core with a native Jetpack Compose interface. It is also available for Windows and Linux.
```

## Spanish store listing (Spanish, Spain)

Play Console → Main store listing → **Manage translations** → add
**Spanish (Spain) – es-ES**, then paste these texts. The graphics can stay
shared with the English listing.

### App name (max 30)

```
Vitela
```

### Short description (max 80)

```
Editor de PDF privado y sin conexión. Edita, rellena, firma y comprime PDF.
```

### Full description (max 4000)

```
Vitela es un editor de PDF rápido y privado que funciona por completo en tu teléfono. Sin cuenta, sin subir nada a la nube, sin anuncios y sin rastreo: la aplicación no tiene permiso de acceso a internet, así que tus documentos nunca salen de tu dispositivo.

BETA: esta es una versión temprana. Puede tener fallos; si algo no funciona, avísanos en el gestor de incidencias del proyecto.

Leer
• Abre PDF desde tus archivos, con «Abrir con», desde el menú de compartir o arrastrándolos desde otra aplicación
• PDF protegidos con contraseña
• Zoom con dos dedos, búsqueda y selección de texto
• Imprime con el diálogo de impresión del sistema

Anotar y editar
• Resalta, subraya y tacha texto
• Dibuja a mano alzada y añade formas
• Mueve, cambia de tamaño y elimina anotaciones, y recórrelas una a una
• Reescribe y mueve texto, también párrafos completos, e inserta texto nuevo
• Pon el texto en negrita, cursiva o subrayado
• Inserta, mueve, cambia de tamaño, sustituye y elimina imágenes
• Añade una marca de agua de texto o de imagen
• Edita los metadatos del documento: título, autor, asunto y palabras clave

Formularios
• Rellena los campos de formulario existentes
• Añade campos de formulario nuevos a un documento

Organizar
• Reordena, gira y elimina páginas
• Combina varios PDF en uno
• Divide un documento en varios archivos

Proteger, firmar y compartir
• Cifra un PDF con contraseña
• Dibuja tu firma con el dedo y, si quieres, recuérdala solo en este teléfono
• Firma con un certificado de un archivo .pfx
• Exporta páginas como imágenes
• Comprime un PDF para que ocupe menos
• Comparte el PDF abierto, con los cambios sin guardar, con cualquier otra aplicación

Disponible en español e inglés.

Vitela es de código abierto y está construido sobre un núcleo común en Rust con una interfaz nativa en Jetpack Compose. También está disponible para Windows y Linux.
```

### Graphics

Everything is in [`play/`](play/). The icon and the feature graphic are
generated from `assets/brand` by `python scripts/play-store-images.py`.

| Asset | Requirement | File |
|---|---|---|
| App icon | 512 × 512 PNG, 32-bit, up to 1 MB | `play/icon-512.png` |
| Feature graphic | 1024 × 500 PNG or JPEG, no transparency | `play/feature-graphic-1024x500.png` |
| Phone screenshots | 2–8, JPEG or 24-bit PNG with no alpha, 16:9 or 9:16, each side 320–3840 px | `play/screenshots/01-home.png` … `05-sign.png`, 1080 × 1920, in upload order |
| Tablet screenshots (7" and 10") | Optional, but needed to be featured on tablets | Not made yet |

The screenshots show a fictional document, *Northwind Annual Report*, on the
`Medium_Phone` emulator, with the display forced to 1080 × 1920
(`adb shell wm size`) and the system UI demo mode on, for a clean status bar.
The emulator's native 1080 × 2400 is 2.22:1, and Play refuses a screenshot
whose long side is more than twice the short one. Never capture a screen that
lists the tester's real files.
