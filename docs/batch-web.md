# Ficha de batch B26 — Shell web (Vitela en el navegador)

> Documentado 2026-11-11, a pedido explícito del usuario. **Fase 0 cerrada (GO, #555);
> fase 1 cerrada: T-217…T-222 ✓ (#560, #561, #569, #571, #572); fase 2 en curso desde
> T-223.**
> Se trabaja en segundo plano, en paralelo al resto de shells. Mismo formato de ficha
> que B24 (compresión).
>
> Origen del scope: el usuario quiere poder usar Vitela desde la web. No existe nada de
> web en el repo — ni crate, ni app, ni spike.

**Dependencias:** B6 ✓ (crates core). Independiente de los demás shells.
Numeración: siguiente libre tras B25. Tareas: T-213 en adelante (T-212 es la última
usada, en `batch-content-edit-v2.md`) — **re-verificar que sigue libre al arrancar**,
hay ramas abiertas.

## Decisiones cerradas

1. **Sin servidor. El core corre en el navegador, compilado a WASM.** El README promete
   offline-first y zero-network, y `tools/zero-network-guard` lo hace cumplir en CI. Un
   backend que reciba PDFs rompería esa garantía. El hosting es estático y el PDF del
   usuario nunca sale de su máquina.

2. **Fachada nueva `core/pdf-wasm` con wasm-bindgen.** `pdf-ffi` está atado a
   `uniffi = "=0.31.0"` (bindings C#/Kotlin) y UniFFI no apunta a wasm. `pdf-wasm` es
   hermana de `pdf-ffi`: misma superficie, otro puente. **Las reglas siguen en el core**;
   la fachada solo expone. Si algo solo existe dentro de `pdf-ffi` y no en los crates de
   dominio, se baja al core antes de exponerlo — no se duplica.

3. **UI en TypeScript + React (Vite), en `apps/web`.** Elegido por el usuario frente a
   Rust/Leptos: ecosistema maduro para canvas, gestos y accesibilidad, y encaja con
   atomic design y container-presentational.

4. **El WASM vive en un Web Worker.** Hace el papel del actor de `pdf-render`: la UI le
   habla por mensajes y el hilo principal nunca se bloquea con un render o un guardado.

## Hechos verificados del repo

- `core/pdf-document` es puro: sin I/O, sin pdfium ni lopdf.
- El `lopdf` vendorizado (`vendor/lopdf-0.45.0`) ya trae la feature `wasm_js`
  (`getrandom/wasm_js`). Los crates sobre lopdf (`pdf-manip`, `pdf-annotate`, `pdf-form`,
  `pdf-edit`, `pdf-compress`, `pdf-save`) son candidatos a portar sin cambios de fondo.
  **Candidatos, no confirmados**: ninguno se ha compilado todavía a `wasm32`.
- `core/pdf-render` usa `pdfium-render 0.9.3` con features `pdfium_7763` y `thread_safe`,
  y es un actor de un solo hilo con cola de prioridad.
- Firma: `pdf-sign-pkcs11` y `pdf-sign-nss` dependen de módulos nativos del sistema.

## Riesgos abiertos (NO verificados)

1. **pdfium en WASM.** No está comprobado que exista un build de pdfium para WASM
   compatible con el pin `pdfium_7763`, ni que la feature `thread_safe` y el actor
   compilen sin hilos. Es el riesgo que decide el batch: por eso la fase 0 es un spike
   go/no-go. Plan B si falla: pdf.js solo para pintar — con el coste de dos motores y el
   riesgo de que render y edición diverjan (el mismo tipo de problema que ya dio el
   doble pintado de campos de formulario).
2. **Tamaño de descarga.** pdfium en WASM pesa varios MB. Se mide en el spike; no se
   asume.
3. **Guardar.** La File System Access API solo existe en navegadores Chromium. En el
   resto, "guardar" es una descarga y no hay "guardar sobre el mismo archivo".
4. **Memoria.** Documentos grandes (el caso de 1619 páginas que ya congeló Linux) contra
   el límite de memoria de wasm32. Sin medir.
5. **Firma.** Solo `.pfx` (`pdf-sign-pfx`) es candidato; pkcs11 y nss quedan fuera.

## Estructura

```
core/pdf-wasm/          fachada wasm-bindgen (cdylib), tests con wasm-bindgen-test
apps/web/
  src/
    main.tsx            solo arranque
    app/                bootstrap, rutas, proveedores
    core/               cliente del worker + tipos compartidos (el "state/models")
    worker/             el Web Worker que carga el WASM
    features/
      document/         abrir, contraseña, guardar
      render/           canvas, zoom, scroll virtualizado
      search/
      organize/
      ...               un módulo por feature, como apps/linux-gtk/src/app
    ui/                 átomos/moléculas de presentación, sin lógica
```

Regla del proyecto (`CLAUDE.md`): **ningún shell monolítico**. Partido por
responsabilidad desde el primer commit; contenedores separados de presentación; los
tests junto al módulo que prueban.

## Fases

| Fase | Entrega | Hecho cuando |
|---|---|---|
| 0 | **Spike de render** (go/no-go) | Una página de un fixture del repo se pinta en un `<canvas>` con pdfium WASM, con tamaño del bundle y tiempo de primer render medidos. |
| 1 | `pdf-wasm` + visor | Abrir (incl. con contraseña), scroll, zoom, búsqueda, selección de texto. |
| 2 | Organize + guardar | Reordenar, rotar, borrar, importar PDFs, descargar. |
| 3 | Anotar + formularios | Paridad con el panel de anotación y el de rellenado. |
| 4 | Editar contenido, comprimir, proteger, exportar imágenes | Paridad con Linux/Windows. |
| 5 | PWA + deploy estático | Instalable y funcional offline tras la primera carga. |

Las tareas T-xxx de cada fase se desglosan al arrancarla; solo la fase 0 se detalla ya,
porque su resultado puede cambiar todas las demás.

### Fase 0 — tareas

- **T-213** Compilar `pdf-document` + `pdf-manip` a `wasm32-unknown-unknown` y abrir un
  fixture desde un test `wasm-bindgen-test`. Valida la mitad lopdf.
- **T-214** Localizar un build de pdfium WASM compatible con `pdfium_7763` y renderizar
  una página a un buffer RGBA desde Rust en wasm. **Aquí está el go/no-go.**
- **T-215** Página HTML mínima (sin React todavía) que pinta ese buffer en un canvas
  desde un Web Worker. Medir tamaño del bundle y tiempo de primer render.
- **T-216** Escribir el resultado en esta ficha: go, o no-go con el plan B evaluado.

El spike vive en `spikes/web-wasm/`, fuera del workspace, igual que `spikes/uniffi-cs`:
sus dependencias experimentales no pueden romper el build ni el lockfile principal.

### Fase 0 — resultado: **GO** (T-213…T-216, PR #555)

Detalle y reproducción en [`spikes/web-wasm/README.md`](../spikes/web-wasm/README.md).
Lo que condiciona la fase 1:

- `pdf-document` + `pdf-manip` compilan a wasm32 solo con la feature `wasm_js` de lopdf.
- pdfium: `bblanchon/pdfium-binaries` `chromium/7763` (`pdfium-wasm.tgz`, sha256 fijado
  en `build.sh`) con `pdfium-render =0.9.5`. Necesita el alias JS
  `Module._PDFium_Init = Module._FPDF_InitLibrary` antes de enlazar — frágil, hay que
  fijarlo con un test o arreglarlo upstream.
- El heap de pdfium **sí crece** hasta 2 GiB (el aviso del README de pdfium-render está
  desactualizado): 500 páginas y un PDF de 75 MB pintaron bien.
- Bundle: 6,5 MiB crudo / 2,7 MiB gzip (pdfium es ~83 %). Primer render ~130–190 ms en
  localhost, solo Chromium.
- **`core/pdf-render` no compila a wasm32** (`bind_to_library`, `library.rs`,
  `load_pdf_from_file`) y su actor no puede funcionar: `thread::spawn` da `Err`,
  `Condvar::wait` hace panic y `JobHandle::wait()` **se cuelga para siempre**. En la web
  el Web Worker es el actor.

### Fase 1 — tareas

Hechos que las fundamentan (mapeados 2026-11-11):

- Todo el trabajo real de `pdf-render` ya está en funciones libres
  `*_job(&mut PdfiumState, …)` (`renderer.rs`: render, tiles, text_runs, page_size,
  page_count, search, page_geometry). `text.rs` y `selection.rs` son puros. Lo atado a
  hilos/rutas es solo `actor.rs`, `library.rs`, `global_actor`/`bind_actor` y
  `open_document(path)`.
- La búsqueda vive en `pdf-render` (`search_job` → `text::find_matches`), exacta y
  sensible a mayúsculas. El permiso de extracción lo aplican los llamadores vía
  `pdf_manip::text_extraction_is_allowed`.
- `pdf-ffi` tiene lógica de dominio que no debe duplicarse en `pdf-wasm`:
  `DocumentState::page_dimensions` (tamaño con rotaciones pendientes) y la orquestación
  de apertura con dos contraseñas (`open_with_passwords_from_bytes`, que elige la de
  propietario para pdfium). El chequeo de permiso + lookup se repite tres veces.

| Tarea | Entrega | Hecho cuando |
|---|---|---|
| **T-217** ✓ #560 | `pdf-render` sin hilos en wasm32: sacar los `*_job` a un módulo `jobs`, extraer `open_from_bytes_job`, y poner actor, `library.rs`, `bind_actor` y la apertura por ruta tras `cfg(not(target_arch = "wasm32"))`. En wasm32, estado en `thread_local` y métodos síncronos. | Nativo: **cero cambio de comportamiento** (tests de `core.yml` en verde). `cargo check --target wasm32-unknown-unknown -p pdf-render` pasa. |
| **T-218** ✓ #561 | Bajar al core la lógica que hoy vive en `pdf-ffi`: dimensiones de página con rotación, apertura con contraseñas y el gate de extracción de texto. `pdf-ffi` pasa a delegar. | Tests movidos junto a la lógica; los shells no notan nada. |
| **T-219** ✓ | `core/pdf-wasm` en el workspace (wasm-bindgen): abrir desde bytes (con/sin contraseña, dual), nº de páginas, dimensiones, render → RGBA, text runs, caracteres de página, búsqueda. | `wasm-bindgen-test` en navegador headless; job wasm32 en CI; cubierto por cargo-audit/deny del root. |
| **T-220** ✓ | Esqueleto `apps/web` (Vite + React + TS): worker que carga ambos WASM (con el alias), cliente tipado del worker, Vitest, Playwright con el test zero-network, `web.yml`. Matriz: Chromium, Firefox y WebKit (decidida 2026-11-11). | Build reproducible; Playwright abre un fixture y ve un canvas pintado; zero-network en verde. |
| **T-221** ✓ | Visor: abrir (selector + soltar archivo), diálogo de contraseña, scroll virtualizado, zoom (incl. rueda anclada al puntero). | Flujos Playwright: abrir normal, abrir cifrado, scroll a la última página, zoom. |
| **T-222** ✓ | Búsqueda y selección de texto (resaltado, copiar), respetando el permiso de extracción. | Flujos Playwright: buscar y saltar entre resultados; seleccionar y copiar; PDF sin permiso → bloqueado. |

T-217 y T-218 son independientes entre sí y bloquean T-219; T-220 puede ir en paralelo
a ellas. Re-verificar al arrancar que T-217…T-222 siguen libres.

**Resultado de T-218 (2026-11-11).** Las tres piezas viven ahora en `pdf-manip`, que no
tiene pdfium: `layout::current_page_dimensions` + `OpenedRotations`,
`open::OpenCredentials` (elige la contraseña de propietario para el renderer) y
`security::ensure_text_extraction_allowed`. La apertura **solo se bajó en parte**:
construir el modelo (`pdf_save::document_from_lopdf`) sigue en `pdf-ffi` porque
**`pdf-save` no compila a wasm32**: `export/mod.rs` usa `pdf_render::PdfiumRenderer`,
que T-217 dejó fuera de wasm32. **Prerrequisito de T-219:** que `pdf-save` compile a
wasm32 (p. ej. `export` tras `cfg(not(target_arch = "wasm32"))`). Los gates de
portapapeles y de `image_source_bytes` no se unificaron porque tienen mensajes propios.

### T-220 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md). Lo que condiciona lo que sigue:

- **Los tres navegadores pintan pdfium WASM.** Playwright abre
  `forms/reportlab_acroform.pdf` y comprueba un canvas de 1200×1553 con tinta en
  Chromium, Firefox y WebKit, y el test zero-network pasa en los tres. Verificado en
  local (Windows, Playwright 1.65); en CI corre en Linux.
- **Motor provisional:** `apps/web/render-wasm/` (wasm-bindgen sobre
  `pdfium-render =0.9.5`, solo abrir + render) hace de `pdf-wasm` hasta T-219. Fuera del
  workspace, con su propio `Cargo.lock` auditado en `security.yml`. Solo lo importa
  `src/worker/engine.ts`: el cambio a `pdf-wasm` queda dentro de `src/worker/`.
- **Segundo workaround de pdfium-render, además del alias.** El worker es un *module
  worker* (Vite), donde `importScripts` no existe, así que el glue de pdfium se evalúa
  como cuerpo de función y no como script global. Así su `wasmTable` deja de ser global,
  y pdfium-render lo busca ahí primero. La solución es exponer `Module.wasmExports`,
  el fallback documentado de pdfium-render (issue #128). El alias y este fallback tienen
  tests, también contra el pdfium real (`src/worker/pdfium.real.test.ts`, en Node).
- **Build reproducible:** el bundle, con la misma toolchain, sale idéntico byte a byte
  tras un build desde cero. El módulo Rust pesa 513 KB crudo / 100 KB gzip (sin
  `pdf-manip`, a diferencia del spike).
- **Zero-network** = ninguna petición después de `data-engine="ready"`, incluidas las
  del worker. El test exige ver el `pdfium.wasm` del worker durante la carga, para
  demostrar que no es ciego en ese navegador. Además se comprobó que falla en los tres
  navegadores si el worker hace un `fetch` tras la carga.

### T-219 — resultado

`core/pdf-wasm` es miembro del workspace y sustituye a `apps/web/render-wasm/`, que se
borró. Lo que condiciona lo que sigue:

- **Superficie:** un único `Engine` (wasm-bindgen) dueño del `SyncRenderer` de T-217 y de
  los documentos abiertos, que se direccionan por id numérico como los handles nativos:
  `open(bytes, password?)`, `openWithPasswords(bytes, user, owner)`, `close`,
  `pageCount`, `pageDimensions` (ancho/alto en puntos + giro), `textExtractionAllowed`,
  `renderPage(doc, page, dpi, invert)` → RGBA sin padding, `textRuns`, `pageCharacters`
  (`caretAt`, `wordAt`, `textIn`, `rectsIn`) y `search`. Los rects cruzan aplanados en un
  `Float32Array` (`x, y, ancho, alto` por rect, origen abajo-izquierda), no como objetos.
- **Misma lógica que `pdf-ffi`, sin duplicarla:** abrir primero el modelo lopdf con
  `pdf_manip::OpenCredentials` y después pdfium con `render_password()`; dimensiones con
  `pdf_manip::current_page_dimensions`; el gate de texto con
  `pdf_manip::ensure_text_extraction_allowed` delante de runs, caracteres y búsqueda.
- **Errores tipados:** todo fallo llega a JS como `Error` cuyo `name` es el tipo
  (`PasswordRequired`, `WrongPassword`, `TextExtractionDenied`, `PageIndexOutOfBounds`,
  `DocumentNotFound`, `InvalidDocument`, `RenderFailed`, `EngineUnavailable`,
  `UnsupportedSecurityHandler`). **Pendiente para T-221:** el protocolo del worker solo
  reenvía `message`; el diálogo de contraseña necesita que también viaje el `name`.
- **El render es por DPI, no por ancho.** El contrato del core (todos los shells) es DPI
  entero, y 1200 px exactos no salen de ningún DPI entero para Letter. El protocolo del
  worker pasó de `targetWidth` a `dpi`; el visor pinta a 144 dpi y el E2E espera
  1224×1584. Es el único cambio fuera de `src/worker/`.
- **Una sola fijación de pdfium:** `core/pdf-wasm/scripts/fetch-pdfium.sh <dir>` (tag +
  sha256); la usan `apps/web/scripts/build-wasm.sh` y los tests del crate.
- **Tests:** 15 unitarios nativos (errores, modelo, conversión) dentro del `cargo test
  --workspace` de `core.yml`; 8 `wasm-bindgen-test` contra el pdfium real en Chrome
  headless, **dentro de un dedicated worker** como en producción, con el glue evaluado
  como cuerpo de función igual que `pdfium.ts`. pdfium entra compilado en el test
  (`PDFIUM_WASM_DIR`), así el servidor del runner no sirve nada más. Se comprobó que fallan
  al romper una aserción. Los PDFs de prueba (texto, `/Rotate 90`, AES con copia
  prohibida) se construyen en memoria con lopdf porque `gen-fixtures` no compila a wasm32.
- **CI:** job `wasm32` nuevo en `core.yml` (check, clippy `--all-targets` y los tests en
  Chrome headless con el chromedriver del runner). `web.yml` ahora también se dispara con
  cambios en `core/`, `vendor/` y `Cargo.*`. `security.yml` ya no audita un lockfile
  aparte: `pdf-wasm` está en el `Cargo.lock` del root, cubierto por cargo-audit/deny (sin
  advisories nuevas). El root pasa de wasm-bindgen 0.2.126 a 0.2.130 (fijado exacto, igual
  que el CLI); solo se compila para wasm32.
- **Tamaño:** perfil `wasm-release` (LTO + 1 CGU) en el root. El módulo pesa 1,56 MB crudo
  / 502 KB gzip, frente a 513 KB / 100 KB del shim: ahora incluye lopdf y el modelo de
  `pdf-save`. pdfium sigue siendo 5,2 MB / 2,4 MB.
- **Verificado en local (Windows):** `cargo check --target wasm32-unknown-unknown -p
  pdf-wasm`, los 8 tests de navegador (Chrome 157 + chromedriver de Chrome for Testing),
  Vitest 30/30 y Playwright 9/9 (Chromium, Firefox, WebKit, incluido zero-network).

### T-221 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("The viewer"). Lo que condiciona
lo que sigue:

- **El protocolo del worker reenvía el `name` del error.** La respuesta `error` lleva
  `name` además de `message`, y el cliente rechaza con un `EngineError` (`core/errors.ts`)
  cuyo `name` es el tipo del core. El diálogo de contraseña ramifica en
  `PasswordRequired`/`WrongPassword`; T-222 debe hacer lo mismo con
  `TextExtractionDenied`, sin tocar el protocolo.
- **`open` devuelve un handle y los tamaños de página** (`{ handle, info }`; `pages`:
  ancho/alto en puntos ya girados + giro, además de `pageCount`), y acepta contraseña.
  **Todo `renderPage` lleva el handle** del documento, y hay un `close(handle)`. El
  handle es el id del core, que nunca se reutiliza y rechaza (`DocumentNotFound`) un id
  cerrado: el worker ya no tiene un documento "actual", así que no puede pintar uno que
  la UI no muestra. Quien cierra es la UI (`useDocument`): el documento que reemplaza en
  cuanto el nuevo abrió, y cualquier open que llega tarde (superado por otro o con el
  diálogo de contraseña cancelado mientras se comprobaba). Un open fallido o cancelado
  deja el documento mostrado intacto. Antes el worker cambiaba de documento al abrir y
  la UI podía seguir mostrando el anterior con páginas del nuevo.
- **Escala:** 100 % = tamaño físico (96 px CSS por pulgada). El render es
  `round(96 × zoom × devicePixelRatio)` dpi, con tope de 12 Mpx por página. Cambia la
  cifra de T-219: a 100 % el E2E ve 816×1056 (1632×2112 en WebKit, cuyo perfil de
  escritorio tiene `devicePixelRatio` 2). **Para T-222:** los rects del core vienen en
  puntos con origen abajo-izquierda; a CSS se pasa con `CSS_PX_PER_PT × zoom` y la
  caja de cada página (`layout.ts`, `slots`).
- **Virtualización:** solo se montan las páginas a menos de un viewport del área
  visible (`visiblePages`); el resto no tiene canvas. Una capa de texto o de resaltados
  de T-222 debe vivir dentro de `PageSlot`, para montarse y desmontarse con la página.
- **Sin renders obsoletos:** el visor entero se reinicia por `generation` al abrir otro
  documento y cada página pide su render por handle; un render que termina después de que cambió el dpi se descarta, y el
  bitmap anterior se muestra escalado hasta que llega el nuevo. El dpi solo cambia
  150 ms después de que el zoom se quedó quieto. Lo que **no** hay: cancelación dentro
  del worker — un render ya encolado se ejecuta aunque su resultado se descarte.
- **Zoom con rueda anclado al puntero:** la matemática es pura (`zoomAround`, con
  tests) y se verificó que el E2E falla en los tres navegadores si se ancla al centro.
  Límite conocido: cerca del borde de la página el punto no puede quedarse quieto si
  el scroll tendría que pasar del borde del contenido.
- **Fixtures:** el cifrado es `encrypted/aes_128_user_and_owner.pdf` (contraseñas en
  `tests/fixtures/README.md`). El corpus no tiene ningún PDF de más de 3 páginas ni con
  `/Rotate 90`, así que el E2E los construye en memoria (`e2e/pdf.ts`): no se commitea
  ningún binario.
- **Verificado en local (Windows, Playwright 1.65):** `tsc -b`, Vitest 79/79, build y Playwright 33/33
  (Chromium, Firefox, WebKit, incluido zero-network), además de 99/99 con
  `--repeat-each=3`.

### T-222 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Search and text selection").
Lo que condiciona lo que sigue:

- **El permiso viene del core y llega dos veces.** `open` devuelve también
  `textExtractionAllowed` (la regla de `pdf_manip`: bit 5 de `/P` o contraseña de
  propietario), y con `false` la UI desactiva búsqueda y Copiar, las páginas no
  aceptan selección y una nota explica por qué. Además cada lectura de texto pasa por
  el gate del core: si el motor rechaza con `TextExtractionDenied` (el `name` que ya
  reenvía el protocolo, sin tocar su forma), el visor apaga lo mismo. Copiar solo
  recibe texto que el core leyó para una selección (`textIn`), así que con el permiso
  denegado no hay nada que copiar.
- **La geometría la coloca el core.** `pdf-wasm` exporta `placeRects` y `pointToPdf`,
  envoltorios planos de `pdf_render::place_rect` / `point_to_pdf` (los que usan los
  shells nativos) a escala 1, y `SearchMatch.lines` (un rect por línea vía
  `line_rects`). El worker devuelve rects sobre la página **tal como se dibuja**
  (origen arriba-izquierda, `/Rotate` aplicado); la UI solo multiplica por
  `CSS_PX_PER_PT × zoom`. No hay matemática de rotación en TypeScript. Los resaltados y
  la selección viven en un `TextLayer` dentro de cada `PageSlot`.
- **Búsqueda:** la del core, exacta y sensible a mayúsculas; TS no compara texto.
  Enter busca, Enter otra vez / ↓ avanza, Shift+Enter / ↑ retrocede, con vuelta al
  principio. Saltar a un resultado calcula el scroll desde el layout (`revealScroll`,
  puro), así que funciona con la página sin montar.
- **Selección:** arrastrar sobre una página o doble clic en una palabra; se queda en la
  página donde empezó (sin selección entre páginas). El worker guarda los caracteres
  de las 4 últimas páginas usadas y los libera al cerrar el documento; la UI tiene una
  sola petición en vuelo y los movimientos intermedios se colapsan en el último.
- **Copiar:** botón Copiar o Ctrl/⌘+C → `navigator.clipboard.writeText`. Ctrl/⌘+C en un
  campo, o sobre texto que el propio navegador tiene seleccionado, se deja al navegador.
- **Hueco del core encontrado (NO arreglado aquí):** un PDF cifrado con contraseña de
  usuario **vacía** (el caso típico "se abre sin pedir nada pero prohíbe copiar") abre
  como **no restringido**. lopdf lo descifra al cargar y quita `/Encrypt`, y
  `pdf_manip::open_document*` toma la salida temprana de `!probe.is_encrypted()` →
  `security: None`. `read_security_context` sí cubre ese caso (`security_from_probe`),
  pero las rutas de apertura no. Verificado en `pdf-wasm` (PDF RC4 construido en
  memoria: `textExtractionAllowed` da `true` y `textRuns` lee el texto); por lectura de
  código, `pdf-ffi` abre por la misma ruta, así que afectaría también a Windows y
  Android. No se arregló en este PR porque cambiar la seguridad que devuelve la apertura
  cambia también el guardado (re-cifrado, gates de reescritura) en todos los shells:
  merece su propia tarea y revisión. Por eso el E2E de "bloqueado" usa contraseña de
  usuario no vacía.
- **Fixtures:** ninguno del corpus prohíbe copiar (los dos de `encrypted/` tienen
  `/P -3884`, bit 5 activo), así que `e2e/pdf.ts` cifra en memoria con el handler
  estándar (RC4 128, R3) y añade líneas de texto en posiciones conocidas.
- **Portapapeles en los E2E:** Chromium y Firefox comprueban el portapapeles real
  pegando en el cuadro de búsqueda. El WebKit de Playwright ni pega ni lee el
  portapapeles, así que ahí la prueba es que `writeText` se llamó con el texto y
  resolvió ("Copied"). Chromium headless necesita `clipboard-read`/`clipboard-write`
  concedidos.
- **Verificado en local (Windows, Playwright 1.65):** `tsc -b`, Vitest 155/155 (incluido
  el test contra pdfium real, que busca y selecciona por `documentEngine` con
  `placeRects`/`pointToPdf` de verdad), build y Playwright 45/45 (Chromium, Firefox,
  WebKit, incluido zero-network); los 4 flujos nuevos además 36/36 con
  `--repeat-each=3`. Rust: `cargo check --target wasm32-unknown-unknown -p pdf-wasm`,
  `cargo test -p pdf-wasm` (20 unitarios nativos, 5 nuevos de colocación), clippy nativo
  y wasm32 `--all-targets` con `-D warnings`, `cargo fmt --check`. **No corrido en
  local:** los `wasm-bindgen-test` en Chrome headless (no hay chromedriver en esta
  máquina); hay uno nuevo (texto en página girada) y otra aserción en el de búsqueda,
  que verifica el job `wasm32` de `core.yml`.
- **Nota posterior:** el hueco de la contraseña de usuario vacía se cerró en el core
  (0874a94, "keep the permissions of PDFs with an empty user password on open").

### Fase 2 — tareas

Hechos que las fundamentan (mapeados 2026-11-12 sobre `origin/main`):

- `pdf-save` ya compila a wasm32 (solo `export_page_as_image` queda fuera). `pdf-wasm`
  ya tiene por documento el `pdf_document::Document` (con su `EditLog`), el lopdf base
  y las rotaciones de apertura, pero **no** aplica ediciones, no deshace, no guarda y
  no conserva los bytes originales (`SaveInput.original_bytes` los necesita).
- La lógica de las ediciones de página ya está en el core (`Command::MovePages`,
  `RotatePages`, `remove_pages`, `EditLog`, `pdf_save::checkpoint`, `derive_blocks`),
  pero varias reglas viven **solo en `pdf-ffi`**: los gates de `apply_edit` (permiso de
  ensamblado, de anotación y `full_rewrite_blocker` para ediciones de estructura, con
  copias propias de `is_document_assembly_command` / `is_page_structure_command`), los
  límites de página de `MovePages`/`RotatePages`, la orquestación de `import_prepared`
  (ids de documento importado, cursor de `PageId`, "elegido dos veces"/"ya importado",
  fuentes importadas en una lista propia en vez de `pdf_save::ImportedSourceRegistry`)
  y los pasos de `refresh_preview` (clonar, quitar anotaciones de sesión, guardar,
  reabrir en pdfium).
- Miniaturas: ningún shell tiene una función aparte; todos llaman a `render_page` a un
  DPI bajo calculado en el shell. En la web el documento de pdfium sale de los bytes
  originales, así que sin un `refreshPreview` las miniaturas quedarían obsoletas tras
  mover, borrar o importar.

| Tarea | Entrega | Hecho cuando |
|---|---|---|
| **T-223** ✓ | Bajar al core la orquestación de edición que hoy vive solo en `pdf-ffi`: gates de `apply_edit` (usando `Command::is_document_assembly_edit` / `is_page_structure_edit`), límites de página de mover/rotar, orquestación de import sobre `ImportedSourceRegistry` y los pasos de `refresh_preview`. `pdf-ffi` pasa a delegar. | Nativo: **cero cambio de comportamiento** (`core.yml` en verde); tests movidos junto a la lógica; `cargo check --target wasm32-unknown-unknown -p pdf-wasm` sigue pasando. |
| **T-224** | `pdf-wasm`: conservar bytes originales, `applyEdit` (mover/rotar/borrar páginas), deshacer/rehacer, `saveToBytes` y `refreshPreview` (reabrir pdfium desde lo guardado). | `wasm-bindgen-test`: editar, guardar, reabrir los bytes y comprobar orden/giro/nº de páginas; errores tipados para permiso denegado y página fuera de rango. |
| **T-225** | Web: Guardar/Descargar. File System Access en Chromium (guardar sobre el mismo archivo), descarga en Firefox/WebKit. | Playwright en los tres navegadores: guardar y reabrir el archivo resultante. |
| **T-226** | Web: pantalla Organize (cuadrícula de miniaturas virtualizada, reordenar arrastrando, rotar, borrar, deshacer/rehacer). | Playwright de punta a punta: reordenar/rotar/borrar, guardar y comprobar el PDF resultante; PDF sin permiso de ensamblado → bloqueado. |
| **T-227** | `pdf-wasm` + web: importar PDFs (preparar + importar, un paso de deshacer por elección) y vista Documentos (bloques). | Playwright: importar un PDF, mover un bloque, guardar y comprobar; cifrado → contraseña. |

T-223 bloquea T-224 y T-227. T-225 depende de `saveToBytes` (T-224). T-226 depende de
T-224 y T-225.

### T-223 — resultado

Cuatro commits `refactor(core)`, uno por pieza; `pdf-ffi` delega en todas y los tests
de fachada de `pdf-ffi` (`page_positions`, `document_blocks`, `import`, `import_batch`,
`smoke`…) siguen sin tocar, como prueba de que el cableado no cambió.

- **Límites de mover/rotar → `pdf-document`.** `Command::move_pages` y
  `Command::rotate_pages` (módulo nuevo `page_range.rs`, no en `edit_log.rs`, que ya
  pasa de 3900 líneas) devuelven `PagePositionOutOfRange { position }` con la misma
  posición que nombraba `pdf-ffi` (origen primero, después destino; rotación vacía →
  origen). Aritmética `checked_add`, así que en wasm32 (`usize` de 32 bits) no hay
  desbordamiento. `pdf-ffi` lo traduce a `PageIndexOutOfBounds`.
- **Gates de `apply_edit` → `pdf_save::check_edit` (`edit_gate.rs`).** Mismo orden
  (anotación, ensamblado, formulario estructural, contenido, reescritura completa) y
  mismas palabras; `EditRefusal` se traduce a `UnsupportedOperation { detail }`.
  `pdf-save` y no `pdf-manip` porque es quien tiene `full_rewrite_blocker`.
  **Lo que no cambió y por qué:** el gate recibe un `EditClass` (qué tipo de edición
  es), no el `Command`, porque `pdf-ffi` lo pregunta **antes** de construir el comando
  — construirlo acuña ids de página y de anotación y puede fallar por posición, y un
  documento que prohíbe la edición tiene que decirlo aunque la posición también esté
  mal. Por eso `pdf-ffi` conserva sus predicados sobre `FfiEditCommand`.
  `EditClass::of(&Command)` clasifica con `is_document_assembly_edit` /
  `is_page_structure_edit` / `is_content_edit` / `is_form_field_edit`, y un test fija
  que ambas clasificaciones coinciden sobre el comando construido.
  **Diferencia entre los predicados:** ninguna sobre lo que `pdf-ffi` construye. Los
  del core son más anchos solo en variantes a las que ningún `FfiEditCommand` llega
  (`ImportPages`, `InsertPages`, `MovePage`, `RemoveImportedPages`), y todas caen en
  la misma clase.
- **Import → `pdf_save::import_sources` + `import_refusal` (`import.rs`), sobre
  `ImportedSourceRegistry`.** El registro pasa a ser el dueño de las fuentes del
  handle (gana un `next_id` comprobado y un `insert` interno al crate); el core hace
  los gates del destino, "nada que importar", la posición, "ya importado", los
  cursores de id de documento y de `PageId`, el único `ImportPages` y registra cada
  fuente solo si el paso se aplicó. Las fuentes llegan como `&mut Option<_>`, así que
  un error deja todo como estaba. Las palabras de `rewrite_refusal` viven ahora en
  `ImportRefusal` (sus dos tests se movieron con ellas). **Se queda en `pdf-ffi`:** el
  "elegido dos veces" (identidad de `Arc<PreparedImport>`; en el core dos `&mut` no
  pueden ser la misma fuente) y el orden global de bloqueo de los mutex. Se pregunta
  antes que "nada que importar", que es equivalente: una elección vacía no repite
  nada. **No es `register`:** las fuentes ya pasaron el permiso de copia en
  `open_import_source_from_bytes`, y los ids se reservan antes de saber si el paso se
  aplica.
- **`refresh_preview` → `pdf_save::session_preview` (`preview.rs`).** Clona, vacía las
  anotaciones de la sesión y guarda con `save_document` (firmas reconocidas en
  silencio); reabrir en pdfium se queda en la fachada. **No se cambió a
  `save_preview`:** ese no recorre `/Annots`, este sí (con el conjunto vacío), así que
  los bytes de una actualización incremental pueden diferir y uno puede rechazar un
  `/Annots` malformado que el otro no lee. Cambiarlo sería un cambio de comportamiento
  para los shells FFI.

Lo que condiciona T-224: `pdf-wasm` tiene que guardar un `ImportedSourceRegistry` por
documento (no una lista propia) y los bytes originales, llamar a `check_edit` antes de
construir cada comando (con `EditClass::of` si construye primero, pero entonces el
orden de errores frente a "fuera de rango" difiere del de `pdf-ffi`), usar
`Command::move_pages`/`rotate_pages`/`remove_pages`, y `session_preview` para
`refreshPreview`. Cambios mínimos fuera del alcance literal: `register` ahora devuelve
`InvalidSaveRequest` al agotar los ids en vez de desbordar (ningún shell lo llama), y
`import_prepared` toma los mutex de las fuentes antes de comprobar la posición (antes,
después); el resultado es el mismo.

**Verificado en local (Windows):** `cargo fmt --all -- --check`; `cargo clippy
--workspace --all-targets --locked -- -D warnings` (sin `linux-gtk`, que no compila en
Windows); `cargo test --workspace --locked -- --skip gtk_ui_` con pdfium vendorizado
(2005 pasados, 0 fallos, 10 ignorados); `cargo check --locked --target
wasm32-unknown-unknown -p pdf-wasm`; `cargo clippy --locked --target
wasm32-unknown-unknown -p pdf-wasm --all-targets -- -D warnings`. **No corrido en
local:** clippy y tests de `linux-gtk` (CI Linux) y los `wasm-bindgen-test` (sin
chromedriver); este cambio no toca ninguno de los dos.

## Verificación

- **Fachada:** `wasm-bindgen-test` en navegador headless.
- **Lógica de UI:** Vitest. TDD estricto, como en el resto del repo.
- **Flujos (E2E): Playwright, obligatorio.** Decisión del usuario (2026-11-11): la web
  **completa** se prueba con Playwright sobre el build real — cada feature que entra
  llega con su flujo de punta a punta (abrir, operar, guardar y comprobar el PDF
  resultante), no solo con tests unitarios. Una fase no se da por cerrada sin sus flujos
  Playwright en verde en CI. Matriz de navegadores (decisión del usuario, 2026-11-11):
  **Chromium, Firefox y WebKit**, los tres en CI.
- **Zero-network:** un test de Playwright que falla si la app hace cualquier petición
  de red después de la carga inicial. Es el equivalente web de `zero-network-guard`.
- **CI:** workflow propio `web.yml`. El core sigue gateado por `core.yml`.

### Nota: E2E automático también para los shells nativos (fuera de B26)

El usuario quiere lo mismo para las apps: **probarlas automáticamente de punta a
punta**, igual que Playwright hará con la web. **La herramienta no está elegida** y no
es parte de este batch — queda apuntado aquí para que no se pierda y merece su propia
ficha. Punto de partida: hoy solo hay recetas manuales por plataforma (xdotool en
Linux/WSLg, UI Automation en Windows, uiautomator en el emulador Android), ninguna
corriendo en CI.

## Fuera de alcance

- Cualquier backend, cuenta de usuario o sincronización.
- Firma con pkcs11/NSS (tokens y almacenes del sistema).
- Convertir páginas web remotas a PDF (ya excluido en el README por la misma garantía).
- Impresión nativa: se delega en el diálogo del navegador.
