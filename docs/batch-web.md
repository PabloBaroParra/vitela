# Ficha de batch B26 — Shell web (Vitela en el navegador)

> Documentado 2026-11-11, a pedido explícito del usuario. **Fase 0 cerrada (GO, #555);
> fase 1 cerrada: T-217…T-222 ✓ (#560, #561, #569, #571, #572); fase 2 cerrada: T-223 ✓ (#574), T-224 ✓, T-225 ✓, T-226 ✓, T-227 ✓ (#620); fase 3 en curso: T-235 ✓.**
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
| 5 | PWA + deploy estático | Instalable y funcional offline tras la primera carga. Hosting decidido (2026-11-17): **Cloudflare Pages**, Direct Upload desde `.github/workflows/web-deploy.yml` (solo cuando el push cambia la web o un crate del que se compila el motor WASM, calculado con `cargo metadata`; Playwright en los tres navegadores sobre el mismo build antes de publicar). |

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
| **T-224** ✓ | `pdf-wasm`: conservar bytes originales, `applyEdit` (mover/rotar/borrar páginas), deshacer/rehacer, `saveToBytes` y `refreshPreview` (reabrir pdfium desde lo guardado). | `wasm-bindgen-test`: editar, guardar, reabrir los bytes y comprobar orden/giro/nº de páginas; errores tipados para permiso denegado y página fuera de rango. |
| **T-225** ✓ | Web: Guardar/Descargar. File System Access en Chromium (guardar sobre el mismo archivo), descarga en Firefox/WebKit. | Playwright en los tres navegadores: guardar y reabrir el archivo resultante. |
| **T-226** ✓ | Web: pantalla Organize (cuadrícula de miniaturas virtualizada, reordenar arrastrando, rotar, borrar, deshacer/rehacer). | Playwright de punta a punta: reordenar/rotar/borrar, guardar y comprobar el PDF resultante; PDF sin permiso de ensamblado → bloqueado. |
| **T-227** ✓ | `pdf-wasm` + web: importar PDFs (preparar + importar, un paso de deshacer por elección) y vista Documentos (bloques). | Playwright: importar un PDF, mover un bloque, guardar y comprobar; cifrado → contraseña. |

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

### T-224 — resultado

API nueva de `Engine` en JS: `EditCommand.movePages(from, count, to)`,
`EditCommand.rotatePages(from, count, deltaDegrees)`,
`EditCommand.removePages(index, count)`, `applyEdit(doc, edit)`, `undo`/`redo`/
`canUndo`/`canRedo(doc)`, `saveToBytes(doc, invalidateSignatures)` y
`refreshPreview(doc)`. Errores nuevos: `EditNotPermitted`, `UnsupportedOperation`,
`SignaturesWouldBeInvalidated`, `SaveFailed`; la posición fuera de rango reutiliza
`PageIndexOutOfBounds`.

- **Módulos.** `model/edit.rs` (reglas: `PageEdit`, su `EditClass`, construir el
  comando, deshacer/rehacer, guardar, bytes de la vista previa) y `engine/edit.rs`
  (frontera JS). El modelo guarda los bytes originales y un `ImportedSourceRegistry`
  (vacío hasta T-227), como pedía T-223.
- **Mismo orden que `pdf-ffi`:** `check_edit` con `PageEdit::class()` antes de
  construir el comando, luego fuera de rango, luego el rechazo del modelo. Un test
  fija que cada variante de `PageEdit` se clasifica igual que `EditClass::of` sobre el
  comando que construye (era la sugerencia abierta de T-223).
- **Diferencias con `pdf-ffi`:** el permiso que falta es `EditNotPermitted` (en
  `pdf-ffi`, `UnsupportedOperation`; mismas palabras); guardar solo con la intención
  por defecto; deshacer/rehacer sobre el `EditLog` sin checkpoints (con solo páginas no
  hay). Como en `pdf-ffi`, `pageCount`/`pageDimensions` cambian al editar pero píxeles,
  texto y búsqueda solo tras `refreshPreview`.
- **Bug encontrado por los tests de navegador:** `SystemTime::now()` hace panic en
  `wasm32-unknown-unknown`, así que **todo guardado** desde la web abortaba
  (`pdf_save::SystemClock` / `RandomIdGenerator`). `clock.rs` lee `js_sys::Date::now()`
  en wasm32 (`js-sys` solo para ese target). Ni `cargo check` ni clippy lo ven.
- **Trampa de tests:** `lopdf::extract_text` en un módulo de test wasm32 en debug no
  carga ("too many locals"); el helper lee el operador de texto de cada página.

**Revisión 4R** (947 líneas): sin bloqueantes. Queda abierto: (1) entre una edición y
`refreshPreview` la vista va desfasada y JS no puede saberlo (`search` puede devolver
una página que ya no existe) — T-226 tiene que refrescar tras cada paso; (2) memoria en
wasm32: una copia más de los bytes originales y picos en el refresco, y quedarse sin
memoria tumba el módulo; (3) en wasm el `/ID` usa milisegundos, dos guardados en el
mismo milisegundo pueden coincidir; (4) el ayudante que avanza el `EditLog` está en tres
sitios (`pdf-ffi`, `pdf-save`, `pdf-wasm`); su sitio sería `pdf-document`; (5) acotar
el `cfg` del reloj con `target_os = "unknown"`; (6) el modelo acepta rotar 45° (también
en `pdf-ffi`, previo).

**Verificado en local (Windows):** `cargo fmt --all -- --check`; clippy de `pdf-wasm`
en wasm32 y nativo y de `pdf-save`, `-D warnings`; `cargo test --locked -p pdf-save
-p pdf-wasm` con pdfium vendorizado (todo en verde); `wasm-bindgen-test` en Chrome
headless con chromedriver 157 de Chrome for Testing: **15 pasados, 0 fallos**.

### T-225 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Saving"). Lo que condiciona
lo que sigue:

- **Protocolo:** `saveToBytes(handle, invalidateSignatures)` cruza el worker como la
  petición `save`; los bytes vuelven por transferencia (como el RGBA de `renderPage`).
  `documentEngine` rechaza con `DocumentNotFound` un documento cerrado, igual que el resto.
- **Tres destinos** (`features/save/`): sobre el mismo archivo si el documento entró con un
  `FileSystemFileHandle`, una **copia** en un archivo nuevo (ver Firmas), y descarga
  (ancla + Blob URL) en cualquier otro caso. El handle se obtiene en las dos puertas por
  las que entra un archivo: `showOpenFilePicker` (el botón Open lo usa si el navegador lo
  tiene; el `<input type=file>` sigue en el DOM como respaldo y destino del teclado) y
  `getAsFileSystemHandle` en un drop, que **tiene que empezar dentro del evento `drop`**
  (el navegador invalida los items después). El drop solo conserva el handle si su nombre
  coincide con el del `File` soltado (si no, se abre sin handle y Save descarga), y solo
  gana el último drop: un token de secuencia más una re-comprobación de "ocupado" tras el
  `await`. Si `showOpenFilePicker` falla (no es un `AbortError`), ese clic cae al
  `<input>` nativo y el siguiente vuelve a usar el selector; un `AbortError` no hace nada.
- **El guardado va en dos pasos** (`delivery.ts`): `prepare` es lo **primero** que ocurre
  en el clic de Save, antes de pedir los bytes al motor, porque la activación de usuario de
  Chromium puede caducar mientras el worker guarda: ahí se pide el permiso `readwrite` del
  handle (o se abre el diálogo de guardar de una copia) y el resultado viaja hasta
  `deliver`. Si el permiso se deniega se guarda igualmente pero se **descarga** y la
  línea de estado lo dice ("Permission to write over the opened file was refused, so X was
  downloaded instead"). Un `write()`/`close()` fallido llama a `writable.abort()` (con
  guarda) antes de caer a la descarga ("Could not write over the opened file…"); nunca se
  descarta en silencio. Chromium escribe en un archivo temporal y reemplaza el original en
  `close()`, así que una escritura fallida deja el archivo del usuario intacto. El adaptador
  de handles vive en `src/platform/` (`fileHandle`, `fileWrite`) para que
  `ui/` y `features/document/` no importen de `features/save/`.
- **Firmas:** ``SignaturesWouldBeInvalidated`` abre `SignatureDialog` (mismas palabras que
  `write-signature-*` del shell Linux, con tres respuestas: **Cancel** —el foco inicial—,
  **Save a copy** y **Save anyway**) y solo entonces reintenta con
  `invalidateSignatures = true`. La web no tiene "Guardar como", así que **Save a copy**
  escribe los bytes reconocidos en un archivo **nuevo** sin tocar nunca el handle del
  archivo abierto: **siempre se descarga** como "nombre (copy).pdf", en los tres navegadores.
  **No usa `showSaveFilePicker`:** Chromium crea y **trunca a cero** el archivo existente que
  el usuario elige antes de devolver el handle (`DidCreateAndTruncateSaveFile` en
  `file_system_access_manager_impl.cc`), así que elegir el propio archivo abierto lo vaciaría
  antes de que cualquier comprobación (`isSameEntry`) pudiera rechazarlo, y un fallo posterior
  del motor dejaría vacío el archivo elegido. Lo encontró la revisión 4R previa a la PR y se
  confirmó en el código de Chromium; una descarga nunca sobrescribe.
  El guardado en curso se acota por documento: uno pendiente de un documento anterior no
  bloquea Save en el nuevo y no entrega nada.
  La copia lleva la firma invalidada; lo que conserva su firma válida es el original, que
  no se toca. **Trampa:** un guardado sin ediciones no reescribe nada, así que un firmado
  sin editar guarda sin preguntar y con los bytes originales como prefijo (el E2E lo
  comprueba). La pregunta no se puede provocar de punta a punta hasta que T-226 meta
  ediciones; hoy la cubren los Vitest de `useSave`, `SaveControls` y `delivery`.
  **Para T-226:** añadir ahí un E2E con un firmado + borrar/rotar página, que cubra Save a
  copy.
- **Cifrados:** sin comportamiento nuevo; `saveToBytes` conserva el cifrado (intención por
  defecto) y el E2E comprueba que el guardado lleva `/Encrypt` y que se reabre con la
  misma contraseña.
- **Dirty:** `useSave` vive en `DocumentWorkspace` (un estado por documento: abrir otro
  empieza limpio, y un guardado que sobrevive a su documento no informa) y entrega
  `markDirty()` por `MarkDirtyContext`: las pantallas de edición de T-226 llaman a
  `useMarkDirty()` (falla en voz alta fuera del workspace). **Solo un guardado que escribió
  sobre el archivo abierto** (`outcome.how === 'wrote'`) limpia el flag, y solo si no entró
  una edición mientras guardaba (contador de ediciones capturado al empezar): una descarga
  (también la de respaldo) o un "Save a copy" dejan el archivo abierto sin las ediciones, así
  que el documento sigue sucio y la línea de estado lo dice ("Downloaded x.pdf. The opened
  file still has unsaved changes."). Si se editó durante un guardado que sí escribió, la línea
  dice "Saved over x.pdf, but edits made while saving are not in it. Unsaved changes." en vez
  de dar todo por guardado. Una edición posterior a un guardado sustituye el mensaje por
  "Unsaved changes.". Nadie lo marca todavía.
- **Archivo cambiado en disco:** al abrir un documento con handle, `platform/fileStamp.ts`
  anota `lastModified` y `size` (`handle.getFile()`, una promesa por handle para que una
  comprobación inmediata la espere). `prepare` —lo primero del clic de Save, antes del motor—
  pide primero el permiso de escritura (la activación del clic sigue fresca) y después los
  vuelve a leer: si difieren **no se escribe** y
  `useSave` pasa a `confirmChangedOnDisk`, que abre `ChangedOnDiskDialog` (mismo estilo que
  `SignatureDialog`, ambos sobre `WarningDialog`: `dialog dialog--warning`, icono, **Cancel**
  enfocado) con **Cancel**, **Save a copy** (descarga; no toca el archivo) y **Overwrite**
  (intención `overwriteAnyway`). El cambio **solo se adopta como referencia tras una
  escritura correcta** (`writeToHandle`): si el permiso se rechaza, se cancela la pregunta de
  firma o la escritura falla, el siguiente Save vuelve a preguntar. Justo antes de
  `createWritable`, `deliver` relee el archivo y lo compara con lo que vio `prepare`
  (`Placement.seen`): si cambió mientras el motor trabajaba no se escribe y se vuelve a
  preguntar (`how: 'changedOnDisk'`; tras un permiso concedido no hace falta activación). Si la
  lectura falla el destino es `unverified` (un tipo propio, no un overwrite disfrazado): se
  descarga y la línea dice "Could not check whether the opened file changed on disk, so x.pdf
  was downloaded instead.". Un fallo de lectura durante un guardado se reintenta en el siguiente; si falló **al abrir**,
  no hay referencia y el documento queda `unverified` (nunca se adopta una lectura posterior). Tras
  aceptar el cambio (Overwrite), el sello aceptado viaja por el reintento de firma (Save anyway). Si la acción es una
  copia, la pregunta de firma solo ofrece **Cancel** y **Save copy anyway** (descarga): nada
  que escriba sobre el archivo abierto. Un handle sin `getFile` (dobles de test) no se comprueba. **Límites:**
  `lastModified`+`size` no detecta un cambio que conserve ambos; y entre la comprobación y el
  `close()` del `writable` queda una ventana de milisegundos (el navegador no ofrece un
  compare-and-swap). **No se maneja un worker del motor colgado:** exigiría reiniciar el worker
  y reabrir el documento; queda fuera de alcance.
- **Foco:** Save se queda enfocable mientras guarda (`aria-disabled` + guarda en el
  manejador; un `disabled` real tira el foco a la página) y recupera el foco al cerrarse
  la pregunta de la firma.
- **Playwright:** `e2e/save.spec.ts` — descarga y reapertura en los tres navegadores;
  cifrado; firmado sin editar; el foco que se queda en Save; y, solo en Chromium, seis flujos con **handles reales del
  sistema de archivos privado del origen** (`navigator.storage.getDirectory()`) entregados
  por `showOpenFilePicker` y por `getAsFileSystemHandle` (Playwright no maneja el diálogo
  nativo), incluidos el respaldo a descarga, el permiso denegado, un drop cuyo handle es de otro archivo
  y un selector que falla. Los tests de escritura cuentan las llamadas a `createWritable`: un
  guardado sin ediciones puede reproducir los bytes originales y, sin eso, no se
  distinguiría "escrito" de "intacto". Cuatro más cubren el archivo cambiado en disco
  (modificado desde fuera tras abrir: aparece el diálogo con Cancel enfocado y Cancel lo deja
  como está; Overwrite escribe y el siguiente guardado no vuelve a preguntar; Save a copy
  descarga sin tocarlo; `getFile` que falla descarga y lo dice). El zero-network ahora también guarda (la descarga
  es una Blob URL, no una petición) y sigue en verde. **No cubierto:** Save a copy de punta a punta (necesita una edición, T-226), el aviso de
  permiso del propio navegador (los E2E lo sustituyen por un handle que responde "denied"), un handle sobre el disco real, y Firefox/WebKit no tienen
  File System Access (solo descarga).

### T-226 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Organize"). Lo que condiciona lo
que sigue:

- **Dos exposiciones finas en `pdf-wasm`** (sin reglas nuevas): `pageIds(doc)` (el `PageId`
  de cada página, en orden: la identidad que sobrevive a mover y borrar, como en Linux) y
  `documentAssemblyAllowed(doc)` (`pdf_manip::document_assembly_is_allowed`, la misma que
  pregunta `check_edit`). Tests nativos en `model.rs` y un `wasm-bindgen-test` nuevo.
- **`@vitela/core`:** `Engine` gana `applyEdit(handle, edit)` (`move`/`rotate`/`remove` de
  un tramo de páginas), `undo` y `redo`, que cruzan el worker como peticiones nuevas y
  responden con un `EditedDocument` (`info` editada + `canUndo`/`canRedo`). `DocumentInfo`
  gana `pageIds` y `documentAssemblyAllowed`; hay constante `EDIT_NOT_PERMITTED`.
- **El refresco va dentro de la edición (nota de T-224).** `documentEngine` llama a
  `refreshPreview` en la misma petición que edita, deshace o rehace: el worker responde
  una petición tras otra, así que ningún render puede caer entre la edición y su refresco.
  También olvida los caracteres de página en caché (describen páginas que pueden haberse
  movido) y recalcula los tamaños con los que coloca el texto. Si el refresco falla, el
  paso se deshace (`undo`, o `redo` para un `undo`) y se relanza el error: la lista de
  páginas que ve la UI nunca discrepa de los píxeles. Efecto secundario conocido: tras
  deshacer una edición fallida, rehacer la ofrece otra vez.
- **Pantalla** (`features/organize/`): conmutador **View / Organize** en la barra; cada
  documento abre en View y cada pantalla se monta de cero al cambiar, así el visor relee la
  lista de páginas editada. `useDocument.revise(handle, edited)` acepta la respuesta solo
  si ese documento sigue en pantalla y devuelve si la aceptó; solo entonces Organize llama
  a `markDirty()` (vía `useMarkDirty`, como pedía T-225), así que una edición que acaba
  tras abrir otro documento no ensucia el nuevo. La respuesta se entrega aunque Organize
  se haya desmontado: el core ya tiene la edición.
- **Selección = un tramo contiguo**, así cada operación es un comando del core y un paso de
  deshacer (como `MovePages` en Linux). Clic selecciona, Mayús+clic extiende.
- **Teclado:** la cuadrícula es un `listbox` con `aria-activedescendant` (el foco no se
  pierde aunque la tarjeta activa se desmonte). Flechas/Inicio/Fin mueven el foco (Mayús
  extiende); **Ctrl+Mayús+flecha** (Cmd+Mayús en Mac) mueve las páginas seleccionadas
  —no Alt+flecha, que es "Atrás" en Chromium y Firefox—; Supr/Retroceso borra; Ctrl+Z,
  Ctrl+Mayús+Z y Ctrl+Y deshacen/rehacen. La barra tiene las mismas acciones con
  `aria-disabled` (no `disabled`, que tiraría el foco).
- **Arrastrar:** eventos de puntero, no drag-and-drop HTML5; el hueco de destino sale del
  layout (`dropSlotAt`: delante de la primera tarjeta cuya mitad queda a la derecha del
  puntero), sin medir el DOM. Autoscroll simple cerca de los bordes.
- **Virtualización por filas** (`gridLayout.ts`): solo se montan, y por tanto se pintan, las
  filas a menos de un viewport del área visible.
- **Miniaturas por `PageId`** (`thumbnailCache.ts`, lección de Linux): clave
  `PageId/giro/dpi`, con LRU de 64 MiB. Mover o borrar no vuelve a pintar nada; rotar
  cambia la clave (sin invalidar a mano) y deshacer la rotación reencuentra el bitmap. Un
  render se pide a través de la caché y se guarda aunque la tarjeta ya no esté. **No se
  pide ningún render mientras una edición está en vuelo:** hasta que responde, la UI tiene
  el orden viejo y un render por posición llegaría a la vista ya editada con la etiqueta de
  otra página. La caché vive mientras la pantalla está montada (volver de View la rehace).
- **Permiso:** con `documentAssemblyAllowed = false` la pantalla lo explica y apaga toda
  edición (botones, teclas, arrastre); un `EditNotPermitted` del core la bloquea igual.
- **Hallazgos sobre el core (no cambiados):** (1) rotar un PDF firmado **no** pregunta: la
  rotación va por el escritor incremental, los bytes firmados quedan como prefijo y el E2E
  lo comprueba; la pregunta de firma solo aparece con mover/borrar (reescritura completa).
  (2) El core acepta borrar **todas** las páginas (el firmado de una página queda con 0 y se
  guarda); Linux tampoco lo impide. Si debe prohibirse, es una regla del core, no de la UI.
- **Playwright** (`e2e/organize.spec.ts`, los tres navegadores): arrastrar + rotar + borrar,
  guardar, reabrir la descarga y comprobar orden, giro y nº de páginas (cada página del PDF
  construido tiene su ancho, y la forma de la miniatura la identifica); reordenar con el
  teclado + deshacer/rehacer + volver a View, guardar y comprobar; PDF cifrado sin permiso de
  ensamblado (ni bit 4 ni bit 11) → bloqueado (clic, tecla y arrastre no cambian nada); y el
  firmado: rotar guarda sin preguntar conservando los bytes firmados, borrar abre la pregunta
  y **Save a copy** descarga "rsa2048_sha256 (copy).pdf" (la nota de T-225).
- **Verificado en local (Windows, Playwright 1.65.1):** `tsc -b` de la app y del paquete;
  Vitest de la app 285/285 y de `@vitela/core` 91/91 (incluido un test nuevo contra pdfium
  real que edita por el `EditCommand` de verdad y pinta la rotación sin refresco aparte);
  build; Playwright 82 pasados / 20 saltados (los de File System Access fuera de Chromium):
  Chromium 34/34, Firefox 24 + 10 saltados, WebKit 24 + 10 saltados, incluido zero-network;
  `organize.spec.ts` además 36/36 con `--repeat-each=3`. Rust: `cargo test -p pdf-wasm`
  (40 nativos), clippy nativo y wasm32 `--all-targets` con `-D warnings`, `cargo fmt
  --check`. **No corrido en local:** los `wasm-bindgen-test` en Chrome headless (no hay
  chromedriver en esta máquina); el nuevo lo verifica el job `wasm32` de `core.yml`. Para
  correr Playwright hubo que instalar los navegadores de Playwright 1.65.1 (chromium-1255);
  la ruta con "ó" no rompió nada.
- **No cubierto:** arrastrar con el dedo (táctil) y el autoscroll durante un arrastre largo
  no tienen E2E; no hay selección no contigua (Ctrl+clic); no se mide memoria con documentos
  grandes (la caché tiene tope, pero cada edición clona y re-guarda la vista previa en el
  worker, el punto 2 abierto de T-224).
- **Para T-227:** importar añade páginas con `PageId` nuevos (el core los acuña), así que la
  caché de miniaturas no necesita cambios; `applyEdit` puede crecer con un `import` que
  responda el mismo `EditedDocument`. La vista Documentos (bloques) puede reutilizar
  `gridLayout`, `useReorderDrag` y `moves.ts` (un bloque es un tramo).

### T-227 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Organize"). Lo que condiciona lo
que sigue:

- **Preparar baja al core (regla de T-223).** El paso "abrir una fuente" vivía solo en
  `pdf-ffi::prepare_import` (abrir con contraseña y permiso de copia, "sin páginas",
  `graft_report` sobre todas las páginas, avisos). Ahora es
  `pdf_save::prepare_source` (`prepared_source.rs`) con su `PrepareError` (las palabras
  "the PDF has no pages" / "too many pages" viajan con él); `pdf-ffi` delega y traduce
  igual que antes (`Open(e)` → la conversión de `ManipError` de siempre, el resto →
  `UnsupportedOperation`). Cero cambio de comportamiento: los tests de fachada de `pdf-ffi`
  (`import`, `import_batch`…) siguen sin tocar y en verde.
- **`pdf-wasm`** (`model/import.rs`, `engine/import.rs`, `dto/import.rs`):
  `prepareImport(bytes, password?)` → `PreparedImport {id, pageCount, warnings}`;
  `discardPrepared(id)`; `importPrepared(doc, ids, index)` → `ImportedPages {pageCount,
  sourceIds}` sobre `pdf_save::import_sources` (un `ImportPages`, un paso de deshacer, las
  fuentes al `ImportedSourceRegistry` del documento); `importRefusal(doc)`
  (`pdf_save::import_refusal`) y `documentBlocks(doc)` (`derive_blocks`, por posición:
  `source` base/blank/imported + `sourceId`, `part`, `start`, `count`, como
  `pdf-ffi::document_blocks`). Las fuentes preparadas viven en el `Engine` bajo un id
  (JS no puede tener un `LopdfDocument`) y no pertenecen a ningún documento hasta
  importarse. **Lo único que queda en la frontera:** esos ids y el "elegido dos veces" sobre
  ellos (como en `pdf-ffi`, el core recibe cada fuente como un `&mut` propio). Una fuente
  que no está (importada, descartada, nunca preparada) es un hueco vacío que el core
  rechaza como "ya importada". Todo o nada: con cualquier error las fuentes vuelven a estar
  preparadas. Mismo orden que `pdf-ffi`: el rechazo del destino antes que la posición.
- **Errores:** `PasswordRequired`/`WrongPassword` de la fuente con su propio tipo (la UI
  pregunta); lo que la fuente rechaza (copia prohibida, firma, XFA, contenido opcional,
  campos en línea) pasa de `InvalidDocument` a `UnsupportedOperation` (como `pdf-ffi`);
  un permiso del destino que falta es `EditNotPermitted` y una reescritura que el cifrado no
  aguanta `UnsupportedOperation`, igual que en `applyEdit`.
- **`@vitela/core`:** `Engine` gana `prepareImport` (los bytes viajan por transferencia,
  como en `open`), `discardImport` e `importPrepared`, que responde un `ImportedDocument`
  (`EditedDocument` + `sourceIds`) con el refresco de la vista previa **dentro de la misma
  petición** y la misma marcha atrás que T-226 si el refresco falla. `DocumentInfo` gana
  `blocks` e `importRefusal`. Un test nuevo contra pdfium real importa, comprueba los
  bloques, pinta la página importada sin refresco aparte, deshace y comprueba que la fuente
  quedó gastada. **Trampa:** wasm-bindgen 0.2.130 tipa `Result<Option<String>>` como
  `string` en el `.d.ts`; en ejecución `None` llega como `undefined` (`importRefusal ?? null`).
- **Web — Documentos:** conmutador **Pages / Documents** en la barra de Organize. La
  cuadrícula, el arrastre, el teclado y la selección son el mismo código sobre *unidades*
  (`units.ts`: una página o un bloque); un tramo de bloques es un tramo de páginas, así que
  mover/rotar/borrar un bloque es el mismo comando de página y un paso de deshacer
  (`moveUnitsToSlot`/`moveUnitsBy` sobre `moves.ts`). `ThumbnailGrid` pasó a ser genérico
  (`renderItem`), con `PageCard` y `BlockCard` sobre `CardThumbnail`; la caché por `PageId`
  no cambió (la miniatura de un bloque es su primera página). Tras mover se selecciona lo
  que cubre las páginas movidas (los bloques pueden fusionarse) y al cambiar de vista se
  conservan las mismas páginas seleccionadas. Nombre del bloque: el archivo abierto, el
  archivo importado (`OpenDocument.sourceNames`, por el `sourceId` que dio la importación;
  el motor nunca ve nombres), "Blank pages", más "(part n)".
- **Web — Add PDFs:** botón en la barra (`FilePicker` con `onPickFiles`, varios archivos) y
  soltar archivos sobre Organize. `useImport` prepara archivo a archivo, pide la contraseña
  de uno cifrado con el mismo `PasswordDialog` (ahora con `action="add"`: "…to add its
  pages", botón **Add**), y añade toda la selección **al final** con un solo
  `importPrepared` (un paso de deshacer); después selecciona las páginas añadidas y marca
  sucio vía `useMarkDirty`. Cancelar una contraseña, un archivo rechazado o salir de la
  pantalla abandona la selección entera y descarta lo preparado. **Soltar:** Organize
  reclama el drop (`preventDefault`) y el listener de ventana de `useFileDrop` ya no abre un
  drop reclamado (lo comprueba `defaultPrevented`); el aviso dice "Drop PDFs to add their
  pages" en Organize. **Gate:** `importRefusal` apaga Add PDFs con el motivo como tooltip (el
  PDF sin permiso de ensamblado del E2E de T-226 ahora lo comprueba también).
- **Playwright** (`e2e/import.spec.ts`, los tres navegadores; los helpers de Organize se
  movieron a `e2e/organize.ts`): importar un PDF, deshacer/rehacer la importación (un
  paso), vista Documentos con dos bloques, arrastrar el bloque importado delante, guardar,
  reabrir la descarga y comprobar orden y nº de páginas por su forma (y que vuelve como un
  solo bloque); un PDF cifrado pide su contraseña, dice cuándo es incorrecta y entra con la
  buena (y se guarda); cancelar la contraseña no añade nada (tampoco el otro archivo de la
  selección); soltar un PDF sobre Organize lo añade en vez de abrirlo. Zero-network ahora
  también importa.
- **Verificado en local (Windows, Playwright 1.65.1):** `cargo test --locked -p pdf-wasm
  -p pdf-save -p pdf-ffi` con pdfium vendorizado (757 pasados, 0 fallos, 7 ignorados;
  `pdf-wasm` 50 nativos, 10 nuevos; 3 nuevos en `pdf-save`); clippy nativo de los tres y wasm32 `--all-targets` de
  `pdf-wasm` con `-D warnings`; `cargo fmt --check`. `tsc -b` de la app y del paquete;
  Vitest de `@vitela/core` 100/100 (incluido el de pdfium real) y de la app 340/340; build;
  Playwright 94 pasados / 20 saltados (los de File System Access fuera de Chromium), incluido
  zero-network; `import.spec.ts` + `organize.spec.ts` además 72/72 con `--repeat-each=3`.
  **No corrido en local:** los `wasm-bindgen-test` en Chrome headless (no hay chromedriver en
  esta máquina; los dos nuevos —importar + mover bloque + guardar, y fuente cifrada— compilan
  con clippy wasm32 y los verifica el job `wasm32` de `core.yml`), ni `linux-gtk` (no toca).
- **Dos arreglos de la revisión:**
  - *Cambiar de vista con una edición en vuelo.* El conmutador Pages/Documents no espera a
    `pending` (no manda nada al motor), pero `edit` calculaba la selección nueva en las
    unidades de la vista en la que *empezó* la edición: con un cambio de vista en medio, un
    índice de bloque se leía como índice de página (o al revés) y el siguiente Delete/Rotate
    podía tocar otras páginas. Ahora, si `viewNow.current` (la vista del último render, como
    ya hacía la importación) no es la del inicio, la selección se traduce con
    `reselect` (`units.ts`, puro: las mismas páginas como las tarjetas de la otra vista que
    las contienen), la misma función que usa el conmutador. Se eligió esto y no apagar el
    conmutador mientras hay edición: el resultado es correcto sea cual sea el momento.
  - *Una importación terminada se perdía con otra edición en vuelo.* Rotar/borrar/mover/
    deshacer siguen vivos mientras se prepara o se pide una contraseña; si uno seguía en
    vuelo al llegar `importPrepared`, el candado de `useOrganize` (`busy`) devolvía `null`
    sin mensaje, se descartaba lo preparado y el estado volvía a reposo aunque el usuario
    hubiera tecleado contraseñas. Ahora la importación **espera su turno**
    (`runInTurn`: aguarda a que termine el paso en vuelo y entonces corre); un clic doble
    sobre una edición se sigue descartando como antes. La importación va detrás de las
    páginas que dejó esa edición: `run` actualiza `pageCount` con la respuesta, porque el
    siguiente paso puede empezar antes de que un render traiga la lista nueva. Si la edición
    esperada falla, su error se queda a la vista (la importación no lo borra); si falla la
    importación, se ve como cualquier fallo de Organize.
  - Tests: Vitest de pantalla (mover en Pages y cambiar a Documents en vuelo, rotar un bloque
    y cambiar a Pages en vuelo, selección que cruza bloques al cambiar de vista, importar con
    un borrado en vuelo), de `useOrganize` (la importación espera y va tras las páginas que
    quedan; conserva el fallo de la edición esperada) y de `reselect`.
- **No cubierto:** elegir dónde insertar (siempre al final, como "append"); avisos del
  injerto antes de importar (se muestran después, en la línea de estado, no se pregunta como
  en Linux); renombrar bloques; un drop sobre Organize con la importación rechazada no hace
  nada más que no abrir el archivo (el botón explica el motivo); arrastrar con el dedo.

### Fase 3 — tareas

| Tarea | Entrega | Hecho cuando |
|---|---|---|
| **T-235** ✓ | Bajar al core lo que hoy vive solo en `pdf-ffi`: construir comandos de anotación (cursor de `AnnotationId`, `replace_annotation`, no-op de `SetAnnotationContents`), sello con `check_edit`, instantánea de anotaciones (posición + maquetado de texto libre), mezcla de `comments`, comando de relleno (`set_field_value`) y `form_fill_allowed`. `pdf-ffi` delega. | Nativo: **cero cambio de comportamiento** (tests de fachada sin tocar, `core.yml` en verde); `cargo check --target wasm32-unknown-unknown -p pdf-wasm`. Sin E2E (no hay UI). |
| **T-236** | `pdf-wasm` + `@vitela/core`: `applyEdit` crece con las ediciones de anotación (añadir marcado/tinta/forma/nota/texto libre/sello; mover/redimensionar/recolorear/retexto/borrar), `annotations`, `comments`, `annotationEditingAllowed`, `freeTextLayout`, `stampPlacement`; `EditedDocument` devuelve la instantánea. | `wasm-bindgen-test`: subrayar, guardar, reabrir con lopdf y encontrar `/Highlight`; deshacer lo quita; sin permiso de anotar → `EditNotPermitted`. Vitest contra pdfium real. |
| **T-237** | Web: capa de anotaciones por página (dibuja la instantánea) y marcado sobre la selección de texto (Resaltar/Subrayar/Tachar en `TextToolbar`). | Playwright (3 navegadores): seleccionar texto, Resaltar, se ve, deshacer/rehacer, guardar, reabrir la descarga y comprobar `/Highlight` con su rect; PDF sin permiso → botones apagados. |
| **T-238** | Web: herramientas de dibujo — tinta, forma, nota (diálogo) y texto libre (diálogo con maquetado del core), color. | Playwright: dibujar cada una con el ratón, guardar, reabrir y comprobar `/Ink`, `/Square`, `/Text`, `/FreeText`; un carácter fuera de WinAnsi deja el diálogo abierto con el error. |
| **T-239** | Web: sellos de imagen (elegir o soltar PNG/JPEG sobre la página, tamaño del core). | Playwright: soltar un PNG, se ve, guardar y comprobar `/Stamp` con su apariencia; deshacer y rehacer lo vuelven a pintar. |
| **T-240** | Web: seleccionar y editar una anotación — mover/redimensionar (asas y teclado), diálogos "Mover a"/"Redimensionar", recolorear, editar texto, borrar. | Playwright: mover arrastrando y con flechas, redimensionar, recolorear, borrar, deshacer; guardar y comprobar rects y color. |
| **T-241** | Web: navegación Anterior/Siguiente y lista de comentarios (incluidos los del archivo, solo lectura) con "Leer nota". | Playwright: Siguiente revela la anotación fuera de pantalla; la lista muestra la nota de un fixture con autor y la de la sesión; funciona con anotar prohibido. |
| **T-242** | `pdf-wasm` + `@vitela/core`: `formFields`, `applyEdit` con `setFieldValue` (refresco **dentro** de la petición, como T-226) y `formFillAllowed`. | `wasm-bindgen-test`: rellenar texto/casilla/radio/lista, guardar, reabrir y leer los valores; un valor imposible → error tipado. |
| **T-243** | Web: panel Formularios (lista de campos, rellenar los cuatro tipos, confirmar al salir del campo, Tab entre campos y revelar el campo en la página). | Playwright: rellenar un AcroForm del repo, la página se repinta con el valor, guardar, reabrir y ver los valores; firmado → rellenar guarda incremental conservando el prefijo; sin permiso → solo lectura. |

T-235 bloquea T-236 y T-242. T-236 bloquea T-237…T-241; T-237 (la capa) bloquea T-238…T-240. T-242 bloquea T-243. Tras T-235 las dos pistas (anotar y formularios) pueden ir en paralelo. Fuera de la fase 3: crear/mover/borrar campos de formulario (autoría).

### T-235 — resultado

Mismo patrón que T-223: las reglas bajan al crate dueño del concepto y `pdf-ffi` se queda
con su vocabulario (posición de página → `PageId`, DTO ↔ tipos del core). Los tests de
fachada de `pdf-ffi` (`tests/*.rs`: `comments`, `freetext`, `forms`, `smoke`…) siguen sin
tocar.

- **Comandos de anotación → `pdf_annotate::session`.** `AnnotationIdCursor` (contador
  desde 0 que nunca reutiliza un id, ni tras deshacer), `add_annotation(ids, page,
  NewAnnotation)` (marcado, forma, tinta, nota, texto libre y sello desde bytes; el id se
  gasta antes de construir, como antes), `change_annotation(set, id, AnnotationChange)`
  (retexto/mover/redimensionar/recolorear → `ReplaceAnnotation`) y
  `remove_annotation(set, id)`. **El no-op va aquí:** `change_annotation` devuelve `None`
  solo para un retexto que deja el texto igual; mover cero sigue registrando un paso, como
  hacía `pdf-ffi`. `AnnotateError` gana `AnnotationNotFound(id)`, que `pdf-ffi` traduce al
  `AnnotationNotFound { annotation_id }` de siempre.
- **Clasificación positiva → `Command::is_annotation_edit`** (`pdf-document`):
  `Add`/`Remove`/`ReplaceAnnotation`. `EditClass::of` pasa de lista de exclusión a
  `is_annotation_edit || is_form_field_edit` (mismo resultado sobre todas las variantes).
  El `is_annotation_command` de `pdf-ffi` se queda (el gate pregunta antes de construir,
  razón de T-223) pero pasa a ser un `match` exhaustivo: un `FfiEditCommand` nuevo no
  compila hasta clasificarse.
- **Sello con `check_edit`.** `insert_image_stamp` pregunta
  `check_edit(security, EditClass::ANNOTATION)` en vez de su copia a mano (mismas palabras,
  "annotation editing is not permitted", y antes de gastar el id) y construye con
  `add_annotation(.., NewAnnotation::Stamp)`.
- **Instantánea → `pdf_annotate::annotation_snapshot(&Document)`**: cada anotación con la
  posición actual de su página y, si es texto libre, el `FreeTextLayout` con el que se
  pintará su `/AP`. Omite (como antes) la de una página que ya no está y el texto libre que
  no se puede maquetar. `pdf-ffi` solo convierte a `FfiAnnotation` y omite las clases sin
  forma en la API.
- **Comentarios → `pdf_save::comments(&Document, &base, &ImportedSourceRegistry)`**: página
  a página, primero los del archivo (base o fuente importada, solo lectura, sin id) y luego
  las notas de la sesión con su `AnnotationId`. `pdf-save` porque es quien tiene el
  registro de importados.
- **Relleno → `pdf_form::fill_field(&FormFieldSet, id, FieldValue)`** (valida una copia y
  registra el valor que sobrevive). `FormError` gana `FieldNotFound(id)`, que `pdf-ffi`
  traduce al `FormFieldNotFound` de siempre; el resto sigue siendo `UnsupportedOperation`.
- **Permisos de formulario → `pdf_save::form_fill_allowed` y
  `form_field_editing_allowed`**, los dos como `check_edit(..).is_ok()` sobre las
  constantes nuevas `EditClass::ANNOTATION` (anotar o rellenar: bit 6) y
  `EditClass::STRUCTURAL_FORM` (bits 6 y 4, sin pregunta de reescritura). Una sola fuente
  para el gate y para la pregunta previa de la UI. `form_field_editing_allowed` de
  `pdf-ffi` delega; la fachada no tiene `form_fill_allowed` (sigue siendo
  `annotation_editing_allowed`, la misma respuesta).
- **Lo que no se movió:** la resolución posición → `PageId` y los DTO (son de la
  fachada); `stamp_placement` ya estaba en el core (`pdf_annotate::stamp_placement`), igual
  que el maquetado (`layout_free_text`); colocar/mover/borrar/renombrar campos (autoría,
  fuera de la fase 3). Único test unitario retocado: el que compara clases FFI/core
  desenvuelve el `Option` nuevo de `build_core_command` (`None` = no-op).

Lo que condiciona T-236/T-242: `pdf-wasm` tiene que depender de `pdf-annotate` y `pdf-form`
(hoy solo de `pdf-save`, `pdf-document`, `pdf-manip`, `pdf-render`), guardar un
`AnnotationIdCursor` por documento junto al `ImportedSourceRegistry`, llamar a
`check_edit` con `EditClass::of` o con `EditClass::ANNOTATION` **antes** de construir (si
construye primero, el orden de errores frente a "página fuera de rango" difiere, como en
T-224), tratar el `None` de `change_annotation` como "nada que registrar" (respuesta sin
paso de deshacer) y exponer `annotation_snapshot`/`comments`/`fill_field`/
`form_fill_allowed` tal cual.

**Verificado en local (Windows):** `cargo test --locked -p pdf-document -p pdf-annotate
-p pdf-form -p pdf-save -p pdf-ffi -p pdf-wasm` con pdfium vendorizado (1150 pasados, 0
fallos, 7 ignorados); `cargo clippy --locked --all-targets -- -D warnings` nativo de esos
seis crates; `cargo check --locked --workspace --all-targets`; `cargo check --locked
--target wasm32-unknown-unknown -p pdf-wasm`; `cargo clippy --locked --target
wasm32-unknown-unknown -p pdf-wasm --all-targets -- -D warnings` (con `PDFIUM_WASM_DIR`
apuntando a `apps/web/generated/pdfium`); `cargo fmt --all -- --check`. **No corrido en
local:** clippy/tests de `linux-gtk` (CI Linux; usa `pdf-annotate`/`pdf-form` y los dos
errores son `#[non_exhaustive]`, así que una variante nueva no rompe sus `match`) y los
`wasm-bindgen-test` (sin chromedriver; este cambio no toca `pdf-wasm`).

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
