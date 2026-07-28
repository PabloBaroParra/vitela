# Fichas de batches B8–B13 — pdf-editor-mvp

> Extraído de los artefactos SDD en engram (2026-07-13): tasks (`sdd/pdf-editor-mvp/tasks`),
> spec (`sdd/pdf-editor-mvp/spec`, obs 2248) y apply-progress (`sdd/pdf-editor-mvp/apply-progress`).
> Estado de partida: B0–B7 completos y verificados; los hallazgos del verify de B7 están todos
> resueltos salvo el WARNING de UX de re-render, diferido por decisión a B8/B9.

## Estado de dependencias (2026-07-13)

| Batch | Depende de | Estado |
|-------|-----------|--------|
| B8 (GTK4) | B6 ✓ (dep directa de crates, bypasea FFI) | **LISTO** |
| B9 (macOS) | B7 ✓ | **LISTO** (paralelo con B10) |
| B10 (Windows) | B7 ✓ + spike T-010 GO ✓ | **LISTO** (paralelo con B9) |
| B11 (verificación) | B8+B9+B10; T-123/T-125 además B15–B19; T-124 además B12 | BLOQUEADO |
| B12 (pdf-sign) | B6 ✓ | **LISTO** (paralelo a todos los shells) |
| B13 (Android) | B7 ✓ (NO depende de B1) | **LISTO** |
| B20 (formularios AcroForm, ver [batch-forms.md](batch-forms.md)) | B6 ✓ | **LISTO** (paralelo a todos los shells; T-141..T-143 de B8 dependen de él) |

B8, B9, B10, B12 y B13 son mutuamente independientes y pueden correr en paralelo.
B14 (iOS) queda fuera de este documento: depende de B9 (reutiliza sus bindings y vistas).

---

## B8 — Shell GTK4 (Linux, dogfood)

**Dependencias:** B6 ✓. Dep directa de crates core — **bypasea B7/FFI** por decisión de design
("GTK4 FFI bypass"). El gap de `/Annots` preexistentes que lo bloqueaba se resolvió en la
remediación pre-B7. Las tareas de formularios T-141–T-143 además requieren B20
([batch-forms.md](batch-forms.md)); el resto de B8 no está gateado por él.

### Tareas
- [ ] T-044 Abrir/render página 1, scroll continuo, zoom fit-width/page/custom. [OpenPDF, NavZoom]
      **(2026-07-19 — parcial: viewer multipágina async + scroll continuo + fit-to-width hechos; zoom page/custom pendiente)**
- [x] T-045 Prompt de contraseña en apertura encriptada + UX de error. [PwdPDF]
- [ ] T-046 Selección de texto + búsqueda doc-wide con matches resaltados y navegables. [TextSelSearch]
      **(2026-07-19 — parcial: búsqueda doc-wide + matches navegables hechos; selección de texto pendiente)**
- [ ] T-047 Toolbar de anotaciones wired a pdf-annotate (7 tipos). [AnnoCreate, AnnoEditDelete]
- [ ] T-048 Keybindings undo/redo → EditLog. [UndoRedo]
- [x] T-049 GtkPrintOperation usando render_page a DPI de impresión. [Print]
- [ ] T-050 gdk::Clipboard paste → stamp_from_image_bytes; rechazar URL-texto, sin fetch. [Clipboard]
- [ ] T-051 Drag-and-drop: abrir PDF / insertar imagen como stamp. [ShortcutsDnD]
- [ ] T-052 Shortcuts estándar C/V/Z/Y/P/S/F/O/N. [ShortcutsDnD]
- [ ] T-053 Bundling pdfium .so + empaquetado (deb/AppImage). [pdfium dist]
- [ ] T-054 linux.yml CI: build + package. [infra]
- [ ] T-141 (dep B20) Modo edición de formularios: colocar campo (texto/checkbox/radio/
      dropdown) sobre el canvas, arrastrar/resize con handles, inspector de estilo
      (fuente standard-14, tamaño, color) → comandos MoveFormField/ResizeFormField/
      RestyleFormField del EditLog. [FormUI, UndoRedo]
- [ ] T-142 (dep B20) Modo relleno: panel lateral de inputs generado desde
      `list_form_fields` (o el FormFieldSet directo — B8 bypasea FFI); tipear emite
      SetFieldValue y dibuja el valor como overlay en vivo sobre el rect del widget
      (sin re-render pdfium; /V + /AP se escriben al guardar). [FormUI]
- [ ] T-143 (dep B20) Tab-order del panel = orden del FormFieldSet; foco en el input
      del panel resalta el widget correspondiente en el canvas y viceversa. [FormUI]

### Criterios de aceptación (spec)
- PDF válido sin cifrar abre y renderiza página 1.
- Contraseña errónea → error claro, sin crash y sin render parcial.
- Zoom al 150% persiste al navegar entre páginas.
- Búsqueda: "invoice" presente en 3 páginas → 3 matches resaltados y navegables.
- Anotación borrada + Ctrl+Z → restaurada desde su inverse data del EditLog; Ctrl+Y re-aplica.
- El strip de protección NO es deshacible (vive en audit log, jamás en EditLog).
- Paste de bitmap → stamp en la posición de pegado; paste de URL-texto → mensaje informativo
  y CERO llamadas de red.
- Imprimir un rango → diálogo nativo GTK, output coincide con el documento renderizado.
- PDF arrastrado a la ventana abre; imagen arrastrada sobre un documento abierto → stamp en
  el punto de drop.

### Gotchas heredados
- El gap de undo de B5 (move/resize/restyle de anotaciones fuera del EditLog) se cierra AQUÍ —
  `Command` es `#[non_exhaustive]` precisamente para poder agregar esas variantes.
- B8 es donde se decide el WARNING de UX de re-render (edits sin guardar requieren
  `save_to_bytes` + reabrir handle) con datos de shell real, no en el vacío.

---

## B9 — Shell macOS (SwiftUI)

**Dependencias:** B7 ✓. Paralelo con B10.

### Tareas
- [ ] T-055 App SwiftUI vía bindings pdf-ffi; abrir/render/scroll/zoom. [OpenPDF, NavZoom]
- [ ] T-056 Prompt de contraseña, selección/búsqueda de texto, toolbar de anotaciones, undo/redo. [PwdPDF, TextSelSearch, AnnoCreate, UndoRedo]
- [ ] T-057 NSPrintOperation vía render_page. [Print]
- [ ] T-058 NSPasteboard paste + drag-and-drop; shortcuts. [Clipboard, ShortcutsDnD]
- [ ] T-059 Bundling .dylib, sign + notarize en macos.yml. [pdfium dist]

### Criterios de aceptación
- La misma batería funcional de B8 (OpenPDF, PwdPDF, NavZoom, TextSelSearch, AnnoCreate,
  UndoRedo, Print, Clipboard, ShortcutsDnD), ejecutada a través de la superficie FFI real.
- macos.yml produce una app firmada y notarizada.
  - **Estado parcial:** hoy macos.yml produce un artefacto de desarrollo con firma
    ad-hoc (necesaria para que corra en Apple Silicon), NO notarizado. La firma de
    distribución + notarize sigue abierta en T-059; el criterio no está cumplido.

### Notas
- macos.yml YA existe con el job `swift-bindings` (T-042) — B9 lo extiende, no crea workflow.
  El job pasó a llamarse `macos-development-artifact` y absorbió la generación de bindings:
  sigue fallando si falta `pdf_ffi.swift`, `pdf_ffiFFI.h` o `pdf_ffiFFI.modulemap`.
- T-055 está cubierto solo en su porción abrir/render/scroll/zoom. La UI real (contraseña,
  selección/búsqueda, anotaciones, undo/redo, print, clipboard) sigue sin empezar.
- Para guardar cifrado tras un edit estructural el shell DEBE ofrecer el flujo
  `openWithPasswords`/`openWithPasswordsFromBytes` (fix post-verify de B7); con una sola
  password solo hay save incremental — `save_to_bytes` devuelve `InvalidSaveRequest` tipado
  en el caso no soportado.
- B14 (iOS) reutilizará estos mismos bindings Swift y una porción sustancial de las vistas:
  diseñar las vistas SwiftUI con esa reutilización en mente.

---

## B10 — Shell Windows (WinUI3 + C#)

**Dependencias:** B7 ✓ + T-010 spike uniffi-bindgen-cs = GO ✓. Paralelo con B9.

### Tareas
- [ ] T-060 App WinUI3 vía bindings C# de uniffi-bindgen-cs; abrir/render/scroll/zoom. [OpenPDF, NavZoom]
      **(2026-07-19 — parcial: abrir/render multipágina + scroll continuo hechos; zoom pendiente)**
- [ ] T-061 Prompt de contraseña, selección/búsqueda, toolbar de anotaciones, undo/redo. [PwdPDF, TextSelSearch, AnnoCreate, UndoRedo]
      **(2026-07-19 — parcial: prompt de contraseña + búsqueda con matches navegables hechos; anotaciones + undo/redo pendientes)**
- [x] T-062 PrintDocument vía render_page. [Print]
- [ ] T-063 WinRT Clipboard/DataPackage paste + drag-and-drop; shortcuts. [Clipboard, ShortcutsDnD]
- [ ] T-064 Bundling .dll + firma Authenticode en windows.yml. [pdfium dist]
      **(2026-07-19 — parcial: build del shell WinUI en CI; firma Authenticode pendiente)**

### Criterios de aceptación
- Misma batería funcional que B8/B9 vía FFI.
- Los errores FFI llegan a C# como excepciones tipadas anidadas — nunca strings crudos
  (contrato validado en el spike T-008).
- windows.yml produce binario firmado con Authenticode.

### Gotchas críticos
- uniffi-bindgen-cs v0.11.0 exige `uniffi = "0.31.0"` EXACTO — ya pineado en
  `core/pdf-ffi/Cargo.toml`; cualquier bump debe actualizar ambos juntos
  (ver spikes/uniffi-cs/README.md).
- El round-trip de buffers ≥8MB cuesta ~20ms de marshaling (p50, hardware real del spike),
  NO "single-digit ms": presupuestarlo en el render loop (~1.5% del budget de 1.5s).

---

## B11 — Verificación transversal

**Dependencias:** B8 + B9 + B10. Las tareas de firma (T-123, T-125) además requieren B15–B19;
T-124 requiere B12.

### Tareas
- [ ] T-065 Checklist manual de round-trip de anotaciones en Acrobat/Preview, pre-release. [AnnoInterop]
- [ ] T-066 Test de paridad estructural: misma op en 2 plataformas, comparar output. [Parity]
- [ ] T-067 Auditoría offline/zero-network completa en todos los shells. [Offline]
- [ ] T-123 (dep B15–B19) Round-trip cross-viewer de firmas criptográficas: Acrobat + un segundo validador independiente. [FirmaCripto, AnnoInterop]
- [ ] T-124 (dep B12) Corpus de PDFs firmados conocidos-buenos (rcgen self-signed), versionado en tests/fixtures: single-signature y segunda-firma-no-invalida-primera. [FirmaCripto]
- [ ] T-125 (dep B15–B19) Smoke tests del contrato CertificateSourcePort por plataforma (incl. variante Secure Enclave en macOS/iOS). [FirmaCripto]

### Criterios de aceptación (spec)
- Anotaciones creadas por la app renderizan en Acrobat/Preview con tipo, posición y contenido
  correctos. Text-note enlaza con `/Popup` + `/Parent` de vuelta — NUNCA `/IRT` (reservado
  para reply threads). Stamps renderizan vía su `/AP` con transparencia (SMask).
- Paridad = equivalencia ESTRUCTURAL en producción (páginas, contenido, anotaciones,
  semántica de metadata); byte-idéntica SOLO en CI con clock e ID-generator deterministas
  inyectados (los PDFs embeben /ModDate e IDs no deterministas por defecto).
- Cero requests salientes monitorizando la red durante cualquier operación MVP, en todos
  los shells.

---

## B12 — Crate pdf-sign

**Dependencias:** B6 ✓ únicamente. Conceptualmente paralelo a B7/B8/B9/B10/B13 — no depende
de ningún shell.

### Tareas
- [x] T-070 **PRIMERA TAREA DEL BATCH — smoke test**: validar un callback interface UniFFI
      SÍNCRONO CON VALOR DE RETORNO (patrón de `sign_digest`). El spike T-009 solo validó
      callbacks async de eventos sin retorno; hay que desriesgar ANTES de construir el resto
      del batch sobre este patrón. [risk-mitigation]
- [x] T-071 Scaffold `core/pdf-sign` con RustCrypto (`cms`, `x509-cert`/`x509-parser`, `der`,
      `spki`, `sha2`, traits `signature`), aislado del resto de core (un build futuro
      "solo visor" no debe arrastrar crypto). [infra]
- [x] T-072 Trait `CertificateSourcePort`: `list_identities() -> Vec<SigningIdentity>`,
      `sign_digest(identity_id, digest, alg) -> Result<Vec<u8>, SignError>`. [FirmaCripto]
- [x] T-073 Builder de campo AcroForm/Sig: placeholder `/Contents` (hex ceros, tamaño
      suficiente) + cálculo de `/ByteRange` (dos rangos alrededor del placeholder). [FirmaCripto]
- [x] T-074 Digest configurable (SHA-256/384/512) sobre los byte ranges. [FirmaCripto]
- [x] T-075 Builder CMS SignedData (PKCS#7): digest firmado + cadena de certificados. [FirmaCripto]
- [x] T-076 Hook hacia el writer incremental de pdf-save:
      `append_signature_bytes(doc_bytes, byte_range, signature_der) -> Vec<u8>`. [FirmaCripto, IncrementalAPI]
- [x] T-077a Adaptador Linux PKCS#11 (`cryptoki`) implementando `CertificateSourcePort`;
      la clave privada nunca sale del token. Crate hoja separado (p. ej.
      `core/pdf-sign-pkcs11`) — `pdf-sign` core no debe arrastrar implementaciones de
      clave privada. [FirmaCripto]
- [x] T-077b Adaptador archivos .p12/.pfx vía `p12-keystore` (Rust puro, mantenido;
      `pkcs12` de RustCrypto descartado: descifrado pendiente upstream, aún pre-release)
      + firma in-process (`rsa`/`p256` con traits `signature`). Mismo crate hoja o
      feature no-default — jamás en `pdf-sign` core. Depende de T-077a solo por
      compartir crate/contrato de tests. [FirmaCripto]
- [x] T-078 Corpus de fixtures firmados conocidos-buenos (rcgen self-signed, uso exclusivo
       de test). [FirmaCripto]
- [x] T-079 Cross-validación: verificación estructural automatizada (hash/ByteRange) en CI +
       checklist manual documentado para validación externa. [FirmaCripto]
- [x] T-080 Tests: corrección de ByteRange, sizing del placeholder, coincidencia de digest,
       segunda-firma-preserva-primera. [FirmaCripto]

### Criterios de aceptación (spec delta)
- Firma anexada EXCLUSIVAMENTE vía incremental update — jamás full rewrite al firmar
  (invalidaría `/ByteRange` de firmas previas).
- Segunda firma NO invalida la primera; ambas verificables independientemente.
- Al abrir un documento firmado: indicador básico pass/fail de validez de hash/estructura.
  Contenido alterado fuera del `/ByteRange` firmado → indicador "inválida" SIN bloquear la
  apertura del documento.
- Sin certificados disponibles en el almacén → mensaje claro, sin crash.
- Firma y verificación 100% offline: cero TSA (RFC 3161), OCSP o CRL por red.
- **Out of scope explícito:** LTV, timestamping, validación profunda de cadena de confianza,
  workflow UX de múltiples firmantes.

### Regla de oro (T-076)
El writer incremental YA existe: `pdf_save::strategy::save_incremental`/`IncrementalWriter`,
backed por `lopdf::IncrementalDocument`. pdf-sign lo REUSA vía un hook de bajo nivel a exponer
en pdf-save (o extendiendo `ObjectSink`) — construir un writer paralelo es un defecto de
arquitectura, no un atajo.

---

## B13 — Shell Android (Kotlin/Jetpack Compose)

**Dependencias:** B7 ✓ únicamente. NO depende de B1 (ese spike gateaba exclusivamente
Windows/C#). Paralelo con B9 y B10.

### Tareas
- [ ] T-081 Scaffold Kotlin/Compose + generación de bindings UniFFI Kotlin en android.yml. [infra]
      **(2026-07-26 — parcial: scaffold Gradle/Compose + `scripts/package-android.sh` genera los
      bindings Kotlin desde el `libpdf_ffi.so` construido; android.yml solo corre los unit tests
      JVM, no genera bindings)**
- [ ] T-082 Cross-compile pdfium .so por ABI (arm64-v8a, armeabi-v7a, x86_64) + empaquetado. [pdfium dist]
      **(2026-07-26 — parcial: cross-compile de pdf-ffi vía cargo-ndk para arm64-v8a y x86_64;
      armeabi-v7a pendiente. PDFium sigue siendo un input externo — el repo no lo vendorea ni
      lo descarga, ver apps/android/README.md)**
- [ ] T-083 SAF: `ContentResolver.openInputStream()` → `open_from_bytes`; guardar vía
      `OutputStream` del mismo Uri → `save_to_bytes`. [ui-android, FileAccessPort]
      **(2026-07-26 — parcial: apertura vía SAF (`OpenDocument`) → `open_from_bytes` hecha;
      el guardado no existe todavía porque no hay edición en el shell)**
- [ ] T-084 Abrir/render página 1, scroll continuo, zoom fit-width/page/custom. [ui-android, OpenPDF, NavZoom]
      **(2026-07-26 — parcial: lector de scroll continuo con ventana de render/caché,
      placeholders dimensionados por media box y fit-to-width con techo de píxeles por
      página + invalidación por rotación; zoom page/custom pendiente)**
- [ ] T-085 Prompt de contraseña en apertura encriptada + manejo de error. [ui-android, PwdPDF]
      **(2026-07-26 — parcial: diálogo de contraseña con reintento y mensaje de error
      diferenciado; el diálogo todavía no se puede cancelar (`onDismissRequest` vacío y sin
      botón de descarte), así que un cifrado sin contraseña conocida deja la app atrapada)**
- [ ] T-086 Selección de texto + búsqueda doc-wide con matches navegables. [ui-android, TextSelSearch]
      **(2026-07-26 — parcial: búsqueda doc-wide + matches navegables hechos; selección de
      texto pendiente — mismo estado que T-046 en GTK4)**
- [ ] T-087 Toolbar táctil wired a pdf-annotate (7 tipos incl. image stamp). [ui-android, AnnoCreate, AnnoEditDelete]
- [ ] T-088 Firma dibujada: trazo táctil → PNG con canal alfa → `stamp_from_image_bytes` en
      el placement_rect. [FirmaDibujada]
- [ ] T-089 Undo/redo vía botones táctiles → EditLog. [ui-android, UndoRedo]
- [ ] T-090 Android PrintManager usando render_page a DPI de impresión. [ui-android, Print]
      **(2026-07-26 — parcial: `PrintDocumentAdapter` wired a PrintManager, pero entrega los
      bytes originales del PDF al spooler en vez de rasterizar con `render_page` a DPI de
      impresión; tampoco honra el rango de páginas pedido. Vale mientras no haya edición —
      en cuanto el shell edite, imprimiría el documento sin los cambios)**
- [ ] T-091 Paste de bitmap desde portapapeles → stamp; rechazar URL-texto sin fetch. [ui-android, Clipboard]
- [ ] T-092 Equivalentes táctiles de drag-and-drop: share-sheet nativo / selector SAF /
      arrastre en split-screen. [ui-android, ShortcutsDnD]
- [ ] T-093 .apk firmado + android.yml CI completo (bindings Kotlin + cross-compile por ABI +
      Gradle + firma). [ui-android, infra]
      **(2026-07-26 — parcial: android.yml existe y corre `:app:testDebugUnitTest` (tests JVM
      puros, sin PDFium); falta build del .apk, cross-compile por ABI y firma. OJO: el CI
      pinea Gradle 8.11.1 mientras el wrapper commiteado pide 9.5.0 y el proyecto usa AGP
      9.3.1 — hay que unificar antes de ampliar el workflow)**

### Criterios de aceptación (spec delta)
- Acceso a archivos EXCLUSIVAMENTE vía Storage Access Framework: el core recibe
  bytes/descriptors, NUNCA rutas crudas — ni al abrir ni al guardar.
- Paridad funcional completa con desktop: los 7 tipos de anotación, undo/redo con el MISMO
  EditLog, impresión nativa (PrintManager), paste de bitmap como stamp.
- Touch-first: toolbar táctil reemplaza los atajos Ctrl/Cmd; share-sheet/SAF/split-screen
  reemplazan el drag-and-drop de escritorio (imagen arrastrada en split-screen → stamp).
- La firma dibujada persiste como annotation estándar (ink o image stamp con /AP) e
  interopera en Acrobat/Preview.
- android.yml produce un .apk firmado instalable en dispositivo compatible.

### Nota clave
T-068 ya está hecho: `open_from_bytes`/`save_to_bytes` son el contrato canónico en pdf-ffi,
incluida la variante dual-password (`open_with_passwords_from_bytes`) para guardar cifrado
tras edits estructurales.
