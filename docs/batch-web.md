# Ficha de batch B26 — Shell web (Vitela en el navegador)

> Documentado 2026-11-11, a pedido explícito del usuario. **Fase 0 cerrada (GO, #555);
> fase 1 cerrada: T-217…T-222 ✓ (#560, #561, #569, #571, #572); fase 2 cerrada: T-223 ✓ (#574), T-224 ✓, T-225 ✓, T-226 ✓, T-227 ✓ (#620); fase 3 en curso: T-235 ✓, T-236 ✓, T-237 ✓, T-238 ✓, T-242 ✓, T-243 ✓.**
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
| E | Visor embebible (`@vitela/viewer`) | Cualquier web lo instala con `npm install` y lo monta con la interfaz completa (ver Fase E). |
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
| **T-236** ✓ | `pdf-wasm` + `@vitela/core`: `applyEdit` crece con las ediciones de anotación (añadir marcado/tinta/forma/nota/texto libre/sello; mover/redimensionar/recolorear/retexto/borrar), `annotations`, `comments`, `annotationEditingAllowed`, `freeTextLayout`, `stampPlacement`; `EditedDocument` devuelve la instantánea. | `wasm-bindgen-test`: subrayar, guardar, reabrir con lopdf y encontrar `/Highlight`; deshacer lo quita; sin permiso de anotar → `EditNotPermitted`. Vitest contra pdfium real. |
| **T-237** ✓ | Web: capa de anotaciones por página (dibuja la instantánea) y marcado sobre la selección de texto (Resaltar/Subrayar/Tachar en `TextToolbar`). | Playwright (3 navegadores): seleccionar texto, Resaltar, se ve, deshacer/rehacer, guardar, reabrir la descarga y comprobar `/Highlight` con su rect; PDF sin permiso → botones apagados. |
| **T-238** ✓ | Web: herramientas de dibujo — tinta, forma, nota (diálogo) y texto libre (diálogo con maquetado del core), color. | Playwright: dibujar cada una con el ratón, guardar, reabrir y comprobar `/Ink`, `/Square`, `/Text`, `/FreeText`; un carácter fuera de WinAnsi deja el diálogo abierto con el error. |
| **T-239** ✓ | Web: sellos de imagen (elegir o soltar PNG/JPEG sobre la página, tamaño del core). | Playwright: soltar un PNG, se ve, guardar y comprobar `/Stamp` con su apariencia; deshacer y rehacer lo vuelven a pintar. |
| **T-240** ✓ | Web: seleccionar y editar una anotación — mover/redimensionar (asas y teclado), diálogos "Mover a"/"Redimensionar", recolorear, editar texto, borrar. | Playwright: mover arrastrando y con flechas, redimensionar, recolorear, borrar, deshacer; guardar y comprobar rects y color. |
| **T-241** ✓ | Web: navegación Anterior/Siguiente y lista de comentarios (incluidos los del archivo, solo lectura) con "Leer nota". | Playwright: Siguiente revela la anotación fuera de pantalla; la lista muestra la nota de un fixture con autor y la de la sesión; funciona con anotar prohibido. |
| **T-242** ✓ | `pdf-wasm` + `@vitela/core`: `formFields`, `applyEdit` con `setFieldValue` (refresco **dentro** de la petición, como T-226) y `formFillAllowed`. | `wasm-bindgen-test`: rellenar texto/casilla/radio/lista, guardar, reabrir y leer los valores; un valor imposible → error tipado. |
| **T-243** ✓ | Web: panel Formularios (lista de campos, rellenar los cuatro tipos, confirmar al salir del campo, Tab entre campos y revelar el campo en la página). | Playwright: rellenar un AcroForm del repo, la página se repinta con el valor, guardar, reabrir y ver los valores; firmado → rellenar guarda incremental conservando el prefijo; sin permiso → solo lectura. |

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

### T-236 — resultado

Detalle de la API en [`packages/vitela-core/README.md`](../packages/vitela-core/README.md).
Lo que condiciona lo que sigue:

- **Una regla nueva baja al core:** `pdf_save::shows_in_session_preview(command)` (en
  `preview.rs`, junto a `session_preview`): un paso cambia lo que muestra la vista previa
  salvo que sea una edición de anotación (`!Command::is_annotation_edit`). Un test fija la
  premisa: los bytes de la vista previa con y sin una anotación de la sesión son iguales.
  `pdf-ffi` no tenía esta regla (allí decide cada shell, con `peek_undo`), así que no hay
  nada que delegar.
- **`pdf-wasm`** (`model/annotation.rs` + `model/annotation/tests.rs`,
  `engine/annotation.rs`, `dto/annotation.rs`, `convert/annotation.rs`): depende de
  `pdf-annotate` (no de `pdf-form`: eso es T-242). Un `AnnotationIdCursor` por documento.
  `AnnotationEdit` = `Add { page, NewAnnotation }` / `Change { id, AnnotationChange }` /
  `Remove { id }` en tipos del core. **Mismo orden que `pdf-ffi`:** `check_edit` con
  `EditClass::ANNOTATION` antes de gastar el id, luego la posición, luego el core
  (`add_annotation`/`change_annotation`/`remove_annotation`), luego el rechazo del modelo; el
  `None` del retexto igual no registra nada. `EditCommand` gana `addHighlight`,
  `addUnderline`, `addStrikeout`, `addShape`, `addInk`, `addNote`, `addFreeText`,
  `addStamp`, `moveAnnotation`, `resizeAnnotation`, `recolorAnnotation`,
  `retextAnnotation`, `removeAnnotation` (geometría en espacio PDF, color `0xRRGGBB`);
  `applyEdit` devuelve el id de la anotación añadida (`number | undefined`). `Engine` gana
  `annotations`, `comments`, `annotationEditingAllowed` (`check_edit(ANNOTATION).is_ok()`,
  la misma respuesta que `pdf_manip::annotation_editing_is_allowed`) y `previewStale`;
  funciones libres `freeTextLayout` y `stampPlacement`. Errores nuevos con su propio tipo,
  como en `pdf-ffi`: `AnnotationNotFound`, `InvalidImage`, `EncodingGap`; `InvalidRect` y un
  cambio que el tipo no admite son `UnsupportedOperation`.
- **Política de refresco (decisión).** El modelo lleva `preview_stale`: aplicar, deshacer y
  rehacer (mirando `peek_undo`/`peek_redo` antes de dar el paso) lo marcan solo si el paso
  `shows_in_session_preview`; importar también; `refreshPreview` lo limpia. `documentEngine`
  refresca dentro de la misma petición **solo si `previewStale`**: añadir o mover una
  anotación no reabre pdfium, y deshacer/rehacer en una historia mixta (páginas +
  anotaciones) refresca exactamente en los pasos de página (test nativo, `wasm-bindgen-test`
  y Vitest). Si el refresco falla, la misma marcha atrás que T-226.
- **Forma del DTO (decisión).** Plano: una clase `AnnotationEntry` para todos los tipos
  (`kind` en texto, `rect`/`points` como `Float64Array` vacíos si el tipo no tiene, `color`
  opcional, `contents`, `layout: FreeTextLines` opcional), `CommentEntry` y
  `FreeTextLines`. `@vitela/core` lo decodifica (`worker/annotations.ts`) a uniones
  etiquetadas (`Annotation` por `kind`, `Comment`, `FreeTextLayout`): un `kind` desconocido o
  sin lo que su tipo necesita se omite; los ausentes pasan a `null` (la trampa de T-227: el
  `.d.ts` tipa `Option<String>` como `string`).
- **Geometría en la página dibujada (decisión).** Como `search`/`selectText`: `PageRect
  {left, top, width, height}` y `PagePoint` con origen arriba a la izquierda y `/Rotate`
  aplicado, así los rects de una selección se marcan tal cual. `PageFrame` usa la
  transformación del core (`placeRects`/`pointToPdf`): un rect por sus dos esquinas, un
  desplazamiento por la parte lineal. Probado contra pdfium real con la página girada 90°
  (marcar y mover vuelven al rect dibujado). La colocación del core es `f32`: el viaje de
  ida y vuelta redondea ~1e-4 pt. **Excepción:** las líneas del texto libre van en el marco
  de la caja y giran con la página (como en cualquier visor). **Orden de rechazos:** una
  página fuera de rango o un id desconocido cruzan sin transformar y el core decide (primero
  el permiso).
- **`@vitela/core`:** `Edit = PageEdit | AnnotationEdit` (`addMarkup` con `markup`
  highlight/underline/strikeout, `addShape`, `addInk`, `addNote`, `addFreeText`,
  `addStamp`, `moveAnnotation`, `resizeAnnotation`, `recolorAnnotation`,
  `retextAnnotation`, `removeAnnotation`). `EditedDocument` gana `annotations` (la
  instantánea, en cada respuesta: un movimiento de página mueve sus anotaciones) y
  `addedAnnotationId`; `DocumentInfo` gana `annotationEditingAllowed`; `Engine` gana
  `annotations`, `comments`, `freeTextLayout(contents, w, h)` y
  `stampPlacement(handle, page, image, anchor)`. Constantes `ENCODING_GAP`,
  `ANNOTATION_NOT_FOUND`, `INVALID_IMAGE`.
- **Marcado de varias líneas: un paso por caja.** El constructor del core lleva un solo
  `rect` (no hay `/QuadPoints` en el modelo) y `Command::Batch` excluye las anotaciones a
  propósito (mutan el modelo). Así que una caja = una anotación = un paso de deshacer, y una
  selección de tres líneas son tres `applyEdit` y tres pasos, igual que Linux. Un solo paso
  pediría `/QuadPoints` en `AnnotationKind` o anotaciones dentro de `Batch`: regla del core,
  no de la web.
- **Correcciones de revisión.** (1) `previewStale` es exacto, no pegajoso
  (`pdf-wasm/src/model/preview.rs`: una marca por paso que se ve, en pila paralela al
  `EditLog`): deshacer un paso nunca mostrado vuelve a "fresco". `documentEngine` pone al día
  una vista previa atrasada antes del paso y solo revierte el paso que registró esa misma
  petición; un deshacer sin nada o una anotación nunca revierten nada. (2) `stampPlacement`
  delega toda la colocación en el core (ancla a espacio PDF → `stampPlacement` → caja
  dibujada): en una página girada 90° el ancla queda en la esquina superior derecha
  dibujada, como `pdf-ffi` (test con pdfium real). (3) Mover/redimensionar una anotación que
  la instantánea no tiene (id desconocido o página eliminada) se rechaza con
  `AnnotationNotFound` sin llegar al core; con el permiso denegado sigue decidiendo el core.
- **Para T-237…T-241:**
  - T-237: `useDocument.revise` hoy guarda `info`/`canUndo`/`canRedo`; tiene que guardar
    también `annotations` (al abrir, lista vacía: la sesión empieza sin anotaciones y las del
    archivo las pinta pdfium). La capa dibuja `Annotation` × zoom; la selección
    (`TextSelection.rects`, `left, top, w, h` por línea) se convierte en un `addMarkup` por
    línea. Deshacer/rehacer del visor deben ir por el motor (ya refresca solo cuando toca).
  - T-238: `freeTextLayout` para el diálogo y `EncodingGap` para dejarlo abierto; tinta como
    lista de `PagePoint`.
  - T-239: `stampPlacement` da la caja (lado mayor 144 pt, la esquina superior izquierda de
    la imagen en el ancla, girada con la página como en `pdf-ffi`) y `addStamp` la usa. El motor **no**
    devuelve la imagen: guardarla por `addedAnnotationId` para repintar (redo vuelve con el
    mismo id; los ids nunca se reutilizan).
  - T-240: recolorear solo marcado/forma/tinta; retexto solo texto libre (una nota no se
    puede retextear en el core); la tinta no se redimensiona. Lo demás → `UnsupportedOperation`.
  - T-241: `comments` trae primero las del archivo (`annotationId: null`, solo lectura) y
    luego las notas de la sesión; responde aunque anotar esté prohibido.
- **Refactor de tests:** el arnés del navegador de `tests/engine.rs` pasó a
  `tests/support/browser.rs` para compartirlo con `tests/annotations.rs` (sin cambio de
  comportamiento). `tests/support/pdfs.rs` gana `commented_document` y un PNG de 1×1.
- **Verificado en local (Windows):** `cargo test --locked --no-fail-fast -p pdf-wasm
  -p pdf-annotate -p pdf-save -p pdf-ffi` con pdfium vendorizado (883 pasados, 0 fallos, 7
  ignorados; `pdf-wasm` 72 nativos, 22 nuevos; 2 nuevos en `pdf-save`); `cargo clippy
  --locked --all-targets -- -D warnings` nativo de esos cuatro; `cargo clippy --locked
  --target wasm32-unknown-unknown -p pdf-wasm --all-targets -- -D warnings`; `cargo fmt --all
  -- --check`. `@vitela/core`: `build:wasm`, `tsc -b`, Vitest 125/125 (incluidos dos nuevos
  contra pdfium real: resaltar sin refresco, guardar, encontrar `/Highlight`, reabrir y ver
  píxeles amarillos pintados por pdfium, deshacer y que el guardado ya no lo tenga; y marcar
  + mover en una página girada 90° + nota en `comments` + `EncodingGap`). App: `tsc -b`,
  Vitest 350/350 (solo fixtures de test retocados: los campos nuevos), build,
  Playwright 94 pasados / 20 saltados (los de File System Access fuera de Chromium; Chromium 38/38, Firefox 28 + 10 saltados, WebKit 28 + 10 saltados), incluido zero-network.
  **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; los seis nuevos de
  `tests/annotations.rs` —el criterio de hecho: resaltar, guardar, reabrir con lopdf y
  encontrar `/Highlight`, deshacer lo quita, sin permiso → `EditNotPermitted`— compilan con
  clippy wasm32 y los verifica el job `wasm32` de `core.yml`), ni `linux-gtk` (no toca).
- **No cubierto:** ninguna UI (T-237…T-241); la imagen de un sello no vuelve del motor; el
  marcado de varias líneas es un paso por línea.

### T-237 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Annotations"). Sin cambios en el
core ni en `@vitela/core`. Lo que condiciona lo que sigue:

- **Módulo nuevo `features/annotations/`:** `geometry.ts` (puro, con tests: la regla del
  subrayado/tachado según el giro, el marco del texto libre, la selección → ediciones),
  `AnnotationLayer.tsx` (presentacional) y `useAnnotationEdits.ts` (contenedor: marcado,
  deshacer, rehacer, permiso, fallo).
- **La capa es un SVG en puntos de la página dibujada** (`viewBox` = ancho × alto de la
  página ya girada), montado en cada `PageSlot` entre el bitmap y la capa de texto, inerte al
  puntero. La instantánea entra tal cual; el zoom es solo el tamaño CSS del SVG. Dibuja todos
  los tipos que decodifica T-236: marcado (resaltado al 45 % con `multiply`; subrayado y
  tachado como regla de 1,5 pt), forma y tinta (1 pt, el borde por defecto del guardado),
  icono de nota (su texto como `title`), texto libre con el maquetado del core (Helvetica,
  negro, recortado a la caja) y un recuadro discontinuo para el sello. Opacidad 0,45 y color
  por defecto (255, 220, 0) como Linux/Android.
- **Página girada:** el subrayado sigue el borde inferior **PDF** del rect (un cuarto de
  vuelta horario lo lleva a la izquierda dibujada), como lo dibuja el archivo guardado y
  Android; Linux lo pinta abajo de la caja dibujada. El texto libre gira con la página en el
  marco propio de la caja (lados intercambiados a 90°/270°).
- **Marcado:** botones Resaltar/Subrayar/Tachar en `TextToolbar` (`aria-disabled`, no
  `disabled`, para no tirar el foco). **Un `addMarkup` por línea y un paso de deshacer por
  línea**, en orden: no hay forma limpia de un solo paso sin tocar el core (`/QuadPoints` o
  anotaciones dentro de `Batch`, ver T-236). Si una línea falla se para; las anteriores
  quedan. La selección se quita cuando las líneas han entrado.
- **Deshacer/Rehacer en el visor:** barra "History" con Undo/Redo y Ctrl/⌘+Z,
  Ctrl/⌘+Mayús+Z, Ctrl/⌘+Y en toda la página salvo en campos de texto (el buscador conserva
  su propio deshacer) o si otro manejador ya tomó la tecla. Van por el motor, así que
  deshacen cualquier paso, también una edición de página hecha en Organize. Los pasos se
  encolan: un deshacer pulsado mientras entra un marcado de tres líneas espera a la última.
- **`useDocument.revise` guarda `annotations`** (opcional en `OpenDocument`: sin definir
  hasta la primera edición, como `canUndo`). Cada respuesta del visor (marcado, deshacer,
  rehacer) pasa por `revise` y, si se aceptó, `markDirty` — igual que Organize.
- **Las posiciones siguen a las páginas:** `usePageRender` gana un `pageKey` (`PageId/giro`).
  Un deshacer que pone otra página en una posición la vuelve a pintar sin mostrar el bitmap
  viejo, y quita la selección y los resultados de búsqueda. Un paso de anotación no cambia
  la clave y no pinta nada.
- **Permiso:** con `annotationEditingAllowed = false` los tres botones quedan apagados y
  descritos (`aria-describedby`) por una nota que lo explica; un `EditNotPermitted` del core
  los apaga igual y muestra el mensaje (`role="alert"`). Seleccionar y copiar siguen
  funcionando (son permisos distintos).
- **Playwright** (`e2e/annotate.spec.ts`, tres navegadores): seleccionar dos líneas
  arrastrando, Resaltar → dos marcas sobre las líneas; Undo/Undo/Redo con botones y
  Ctrl+Mayús+Z/Ctrl+Z/Ctrl+Y con el teclado; guardar, leer la descarga y encontrar dos
  `/Highlight` con su `/Rect` en espacio PDF alrededor de la línea base de cada una; reabrir
  la descarga: capa de sesión vacía y píxeles amarillos pintados por pdfium. Subrayar y
  tachar una palabra (doble clic) y encontrar `/Underline` y `/StrikeOut`. PDF RC4 sin bit 6
  (`/P` construido en `e2e/pdf.ts`): se selecciona y copia, pero los botones están apagados
  con su descripción y un clic no añade nada ni ensucia el documento. `e2e/pdfObjects.ts`
  lee los diccionarios de anotación del PDF guardado (objetos sueltos y flujos Flate) sin
  librería.
- **Verificado en local (Windows, Playwright 1.65.1):** app: `tsc -b`, Vitest 388/388 (50
  archivos), build; Playwright 103 pasados / 20 saltados (los de File System Access fuera de
  Chromium): Chromium 41/41, Firefox 31 + 10 saltados, WebKit 31 + 10 saltados, incluido
  zero-network; `annotate.spec.ts` además 27/27 con `--repeat-each=3`. `@vitela/core` (sin
  cambios): `build:wasm`, `tsc -b`, Vitest 132/132. Sin Rust tocado.
- **Correcciones de revisión:** (1) los botones de marcado se apagan (`aria-disabled`)
  mientras corre cualquier edición, deshacer o rehacer (`pending`), `markup` se rechaza en
  ese caso y, justo antes de su primera línea, comprueba que en esa posición sigue la misma
  página (`pageKey` = `PageId/giro`, leído del último resultado del core, por delante del
  re-render); si cambió no marca nada y lo explica. (2) `markup` devuelve si entraron todas
  las líneas; la selección solo se quita si entraron y si sigue siendo la que se marcó (una
  rechazada o fallida la conserva, una nueva hecha mientras tanto también). (3) Las teclas
  de historial se leen en un único módulo, `features/document/historyKeys.ts`
  (`historyAction`, `historyShortcuts`, `isApplePlatform`), usado por el visor
  (`viewer/useUndoKeys.ts`, renombrado) y por Organize, con las mismas
  `aria-keyshortcuts` en ambas barras: no se toman con un diálogo modal abierto, y en Mac
  (según `navigator.platform`, no el user agent) ⌘+Y deja de ser rehacer (es el Historial
  del navegador); Ctrl+Y sigue fuera de Mac. El E2E pulsa Ctrl+Y solo fuera de Mac. (4)
  Fuera `AnnotationEditing` y `marking`. Verificado: `tsc -b`, Vitest 401/401 (51 archivos),
  build; Playwright `annotate` + `organize` + `viewer` 33/33 en los tres navegadores.
- **No cubierto:** el E2E solo dibuja marcado (el resto de tipos de la capa —tinta, forma,
  nota, texto libre, sello— tiene Vitest de componente y llegará a E2E con sus herramientas en
  T-238/T-239); la capa en una página girada solo tiene tests unitarios; no hay selección ni
  edición de una anotación (T-240); el marcado no tiene selector de color.
- **Para T-238…T-241:**
  - T-238: las herramientas de dibujo añaden por el mismo `useAnnotationEdits` (generalizar
    `markup` a un `add(edit)` encolado; mantener la regla "un clic, un paso"). La capa ya
    dibuja `/Ink`, `/Square`, `/Text` y `/FreeText`; un trazo en curso puede ser otro
    `polyline` en el mismo SVG (está en puntos: `toPagePoint` de `text/pageRects.ts` da el
    punto). La capa es `pointer-events: none` y la capa de texto recibe el puntero: un
    modo "herramienta armada" debe decidir quién lo recibe. `EncodingGap` llega como
    `failure` si no se trata antes en el diálogo.
  - T-239: el sello se dibuja hoy como recuadro (`annotation-mark--stamp`); guardar la imagen
    por `addedAnnotationId` (redo vuelve con el mismo id) y pintarla con `<image>` en el
    marco girado, como el texto libre (`freeTextFrame`).
  - T-240: cada marca lleva `data-annotation-id`/`data-kind`; para seleccionar hay que darle
    `pointer-events` a las marcas (o hacer *hit-testing* con la instantánea en la capa de
    texto). Las ediciones (`moveAnnotation`, …) van por la misma cola.
  - T-241: el `title` de la nota es provisional; la lista de comentarios sale de `comments`.
  - Deshacer/rehacer del visor y de Organize comparten la historia del core pero no la
    cola: Organize no se monta a la vez que el visor, así que no compiten.

### T-238 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Drawing tools"). Sin cambios en el
core ni en `@vitela/core`. Lo que condiciona lo que sigue:

- **Archivos nuevos en `features/annotations/`:** `drawing.ts` (puro, con tests: herramientas,
  paleta, umbral de arrastre, caja trazada/de clic dentro de la página, trazo, gesto → edición),
  `DrawingLayer.tsx` (la superficie de la herramienta armada), `DrawToolbar.tsx` (barra "Draw"),
  `useDrawing.ts` (contenedor: herramienta, color, borrador de nota/texto libre, Escape),
  `TextDialog.tsx` + `DraftDialog.tsx` + `FreeTextPreview.tsx` + `useFreeTextPreview.ts` (los
  diálogos y la vista previa del maquetado). `useAnnotationEdits` gana `add(edit, page,
  pageKey, shownByCaller)`: un paso por la misma cola que el marcado.
- **Modo herramienta (decisión).** Select (por defecto) / Ink / Shape / Note / Free text con
  `aria-pressed`, y cinco colores para tinta y forma (el primero, el amarillo por defecto del
  core; nota y texto libre no tienen color en el core). Armar una herramienta monta una
  `DrawingLayer` encima de todas las capas de cada `PageSlot` (eventos de puntero con
  *capture*, `touch-action: none`), la capa de texto deja de recibir el puntero y la selección
  se quita. La herramienta **sigue armada** tras cada gesto (varios trazos forman un dibujo;
  Android vuelve a Pointer tras colocar); Escape en cualquier sitio, salvo en un campo de texto
  o con un diálogo abierto, vuelve a Select.
- **Geometría:** el puntero dividido por el zoom (`toPagePoint`) ya es un punto de la página
  dibujada; el core lo pasa a espacio PDF con cualquier `/Rotate`. Tamaños de los shells
  nativos: umbral de arrastre 8 pt (distancia, no por eje); forma trazada (lado mínimo 4 pt) o
  144 × 36 pt con un clic; nota 24 pt con la esquina en el clic; texto libre trazado (mínimo del
  core 16 × 17,8 pt) o 200 × 50 pt con un clic; todo deslizado dentro de la página. Los dos
  tamaños del texto libre son del marco propio de la caja: en una página `/Rotate 90`/`270` la
  caja dibujada los lleva girados (`drawnSize`), así el marco siempre tiene al menos una línea. La tinta es
  un `addInk` por trazo, puntos a menos de 0,5 pt del anterior fuera, recortados a la página.
- **Nota y texto libre:** el gesto solo elige la caja; nada llega al core hasta "Add" en un
  `<dialog>` modal con el área de texto enfocada (Ctrl/⌘+Enter añade, Escape cancela; un texto
  en blanco no se puede añadir). Mientras el "Add" está en curso la edición ya está en la cola:
  Cancel queda deshabilitado y Escape se ignora hasta que el core responde; si el navegador
  cierra el diálogo por su cuenta (Chromium con un segundo Escape seguido), se vuelve a abrir
  mientras el borrador siga en pantalla. El de texto libre pide `freeTextLayout` al core al pararse la
  escritura (120 ms; una respuesta para un texto ya cambiado se descarta) y lo dibuja en el
  marco propio de la caja (lados cambiados a 90°/270°): el mismo maquetado que el guardado, y un
  carácter fuera de WinAnsi se ve al teclearlo. Si el core rechaza el "Add" con `EncodingGap`,
  el diálogo sigue abierto con su mensaje (no pasa a `failure` del visor); cualquier otro fallo
  lo cierra y lo muestra el visor.
- **Rechazo de un texto ya cambiado (fix posterior).** El área de texto sigue editable
  mientras el "Add" está en curso, y la vista previa ya muestra el mismo mensaje de
  `EncodingGap`: si se reescribía antes de que el core respondiera, el rechazo del texto viejo
  llegaba después y volvía a poner la alerta sobre el texto nuevo (`draw.spec.ts` intermitente
  en Chromium, 1/40–2/10, una vez en CI). `useDrawing` y `useAnnotationEditing` (retextear un
  texto libre) cuentan las reescrituras y descartan el mensaje si hubo alguna desde el envío;
  el diálogo sigue abierto, sin error. Vitest de las respuestas fuera de orden en
  `useDrawing.test.tsx` y `annotationEditing.test.tsx`; forzando la carrera en el navegador
  (rechazo retenido hasta el siguiente `input`) `draw.spec.ts` fallaba siempre en la línea
  101 y pasa con el fix; sin forzarla, 120/120 en los tres navegadores con `--repeat-each=20`.
- **Guardas:** la misma que el marcado: rechazado mientras corre un paso (los botones de
  herramienta quedan `aria-disabled` y la capa no empieza gesto, `data-busy`), y comprobado
  justo antes de aplicar contra la página en la que **empezó** el gesto (`pageKey` tomado al
  pulsar); si cambió, nada y el aviso. Una tinta o forma rechazada porque empezó un paso a mitad
  del gesto (p. ej. Ctrl/⌘+Z) también avisa en el mismo sitio (`notify` de
  `useAnnotationEdits`). Un `pointercancel` o una captura de puntero perdida
  (`lostpointercapture`) descartan el gesto sin dibujar nada. Sin permiso de anotar, Ink/Shape/Note/Free text quedan
  `aria-disabled` y descritos por una nota; un `EditNotPermitted` del core devuelve a Select y
  cierra un diálogo abierto. Select siempre funciona.
- **`historyKeys.ts`** exporta `isTextField` y `modalOpen` (el Escape de las herramientas lee
  las mismas reglas que las teclas de historial).
- **Playwright** (`e2e/draw.spec.ts`, tres navegadores): tinta en azul, rectángulo arrastrado,
  nota con clic + diálogo, texto libre arrastrado + diálogo; `Total 日本` muestra el error al
  teclearlo y "Add" deja el diálogo abierto con `'日' cannot be shown in Helvetica (WinAnsi)`;
  con texto válido la vista previa muestra la línea y se añade. Escape → Select; deshacer y
  rehacer el texto libre; guardar y encontrar `/Ink`, `/Square`, `/Text` y `/FreeText` con sus
  `/Rect` en espacio PDF; reabrir la descarga: capa de sesión vacía y pdfium los pinta (más
  píxeles con tinta que la página en blanco). Segundo test: un rectángulo en una página
  `/Rotate 90` cae en el archivo donde se dibujó. `annotate.spec.ts` comprueba además que las
  herramientas quedan apagadas y descritas en el PDF sin permiso.
- **Verificado en local (Windows, Playwright 1.65.1):** app: `tsc -b`, Vitest 448/448 (56
  archivos), build; Playwright 109 pasados / 20 saltados (los de File System Access fuera de
  Chromium): Chromium 43/43, Firefox 33 + 10 saltados, WebKit 33 + 10 saltados, incluido
  zero-network; `draw.spec.ts` además 18/18 con `--repeat-each=3`. Sin cambios en
  `@vitela/core` ni en Rust.
- **No cubierto:** sin selector de forma (solo rectángulo: el core tiene `/Square`); sin grosor
  de tinta (1 pt, el borde por defecto del guardado); sin atajos de teclado para las
  herramientas ni forma de dibujar sin puntero (nota y texto libre necesitan un clic en la
  página); la nota no tiene vista previa (es un icono); la página girada solo tiene E2E para la
  forma (el resto, Vitest de geometría); no se puede editar una anotación ya dibujada (T-240).
- **Para T-239…T-241:**
  - T-239: el sello puede ser otra herramienta armada (`ArmedTool` + `finishGesture`: un clic da
    el ancla para `stampPlacement`) que añada por `add`; soltar un archivo sobre la página
    necesita el `pageKey` del punto, como un gesto.
  - T-240: Select es hoy "seleccionar texto"; seleccionar una anotación puede vivir en ese
    mismo modo (*hit-testing* con la instantánea antes de empezar una selección de texto) o
    ser otro modo de `DRAW_TOOLS`. `TextDialog` sirve para retextear el texto libre (con
    `useFreeTextPreview`); la paleta de `drawing.ts` para recolorear.
  - T-241: la nota de la sesión lleva su texto en `contents`; el diálogo de lectura puede
    reutilizar `TextDialog` en solo lectura.

### T-239 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Image stamps"). Sin cambios en el
core ni en `@vitela/core`. Lo que condiciona lo que sigue:

- **Archivos nuevos en `features/annotations/`:** `stamps.ts` (puro, con tests: firma PNG/JPEG,
  `data:` URL, qué archivo de un *drop* se sella, la edición `addStamp`) y `useStamps.ts`
  (contenedor: la imagen elegida, sellar con clic o con *drop*). `drawing.ts` gana la
  herramienta `stamp` y un gesto `point` (el punto pulsado, dentro de la página);
  `useDrawing` pasa ese punto a `onStamp`. `e2e/png.ts` construye los PNG en memoria (como
  `e2e/pdf.ts`; no hay PNG en `tests/fixtures`).
- **Dos entradas (decisión).** **Image** en la barra "Draw" abre un selector PNG/JPEG
  (`<input type=file>` oculto, etiqueta "Stamp image"); cuando el archivo se lee arma la
  herramienta `stamp` y **cada clic** en una página sella esa imagen (sigue armada, como las
  demás; Escape → Select). Un PNG/JPEG **soltado sobre una página** se sella en el punto del
  *drop* en cualquier modo, sin armar nada: el `PageSlot` reclama el *drop*
  (`preventDefault`) y el oyente de la ventana no abre nada. Un *drop* que trae un PDF sigue
  siendo del workspace (lo abre). Un archivo de más de `MAX_STAMP_BYTES` (20 MB) se rechaza
  por `File.size` **antes de leerlo**; después la firma de los bytes se comprueba antes de
  llamar al core; lo demás se rechaza con aviso ("Only PNG and JPEG…" solo para un formato
  que no es PNG/JPEG; un archivo que no se puede leer avisa "The image could not be read…").
  De varios *picks* solo vale el último aunque las lecturas terminen desordenadas
  (`useStamps`), y solo arma `stamp` si el usuario no eligió otra herramienta (ni pulsó
  Escape) mientras se leía (`useDrawing.pickImage`). El punto del *drop* se recorta a la
  página con el mismo `clampPoint` que el clic.
- **Colocación: toda del core.** Ancla (clic o *drop*, puntos de la página dibujada) →
  `stampPlacement` → `addStamp`, **los dos dentro del mismo paso** de
  `useAnnotationEdits.add` (que ahora acepta una función que construye la edición dentro del
  paso, tras comprobar la página): un paso de deshacer, comprobado contra la página en la que
  se pulsó (`pageKey`). **Decisión (revisión):** un doble clic rápido da **un** sello y el
  segundo clic se rechaza **antes de llamar al motor**, igual que un segundo trazo de tinta o
  forma: `add` rechaza mientras corre cualquier paso (regla un gesto = un paso, `runningNow`)
  y la capa ignora pulsaciones con `busy`. Antes, `stampPlacement` corría fuera de la cola, la
  página no estaba ocupada y el segundo clic llegaba a `add` a mitad del primero y se
  rechazaba con "Another change was still being made" tras haber preguntado al core.
- **La imagen no vuelve del motor:** se guarda como `data:` URL (sin petición —zero-network—
  y sin nada que revocar) en `OpenDocument.stampImages` por `addedAnnotationId`
  (`useDocument.keepStampImage`, como `sourceNames`). `add` gana un `onAdded(id)` que se llama
  **en el mismo paso, antes de `onEdited`**: la capa nunca pinta el sello sin su imagen. Vive en
  el documento y no en el visor, así que sobrevive a deshacer/rehacer (mismo id; los ids no
  se reutilizan) y a ir a Organize y volver (el visor se vuelve a montar). Abrir otro
  documento la descarta.
- **Capa:** el sello con imagen es un `<image preserveAspectRatio="none">` en el marco propio
  de la caja (`freeTextFrame`: lados cambiados y girado con la página, como la apariencia
  guardada); sin imagen guardada sigue el recuadro discontinuo (`annotation-mark--stamp`).
- **Permiso y fallos:** sin permiso de anotar, Image queda `aria-disabled`, descrita por la
  misma nota (ahora "…Ink, Shape, Note, Free text and Image…"), con el selector deshabilitado;
  un *drop* se rechaza con "This PDF does not allow annotations, so the image was not added."
  y no abre el PNG. `InvalidImage` de `stampPlacement` falla el paso y lo muestra el visor
  como cualquier error del core; un archivo que no es PNG/JPEG, uno que no se lee, uno
  demasiado grande y un sello rechazado porque corría otro paso se avisan en el mismo sitio
  (`notify`).
- **Playwright** (`e2e/stamp.spec.ts`, tres navegadores): soltar un PNG 2:1 en (100, 150) → se
  ve con su `data:` URL, 144 × 72 pt con la esquina en el punto; elegir un PNG 1:2 con Image y
  hacer clic → segundo sello; Escape → Select; deshacer dos veces quita ambos y rehacer dos
  veces los vuelve a pintar **con imagen** (ningún recuadro de reserva); guardar y encontrar
  dos `/Stamp` con su `/Rect` en espacio PDF y `/AP << /N … R >>`, y los dos `/Image` con su
  `/Width`/`/Height`; reabrir la descarga: capa de sesión vacía y pdfium los pinta. Segundo
  test: en una página `/Rotate 90` el sello lleva `rotate(90)`, 72 × 144 dibujado, con el ancla
  en la esquina superior derecha dibujada. `annotate.spec.ts` comprueba además Image apagada y
  el *drop* rechazado en el PDF sin permiso.
- **Verificado en local (Windows, Playwright 1.65.1):** app: `tsc -b`, Vitest 490/490 (58
  archivos; 25 tests nuevos), build; Playwright 115 pasados / 20 saltados (los de File System
  Access fuera de Chromium): Chromium 45/45, Firefox 35 + 10 saltados, WebKit 35 + 10
  saltados, incluido zero-network; `stamp.spec.ts` además 18/18 con `--repeat-each=3`. Sin
  cambios en `@vitela/core` ni en Rust.
  Tras la revisión: `npm run typecheck` limpio, Vitest 499/499 (58 archivos), build;
  Playwright igual (115 pasados / 20 saltados; Chromium 45, Firefox 35 + 10, WebKit 35 + 10) y
  `stamp.spec.ts --repeat-each=3` 18/18.
- **Revisión de T-239 (en el árbol de trabajo):** `PlaceStamp`/`ImageDrop`/`PickStamp` viven
  junto a `useStamps`, que devuelve los manejadores listos (`place`, `drop`, `pick`);
  `DocumentViewer` ya no tiene cableado de sellos (`useDrawing(editing, rotationOf, stamps)`,
  `onImage={drawing.pickImage}`, `onImageDrop={stamps.drop}`). Tests nuevos: Vitest para el
  paso con edición diferida (`useAnnotationEdits`), lectura fallida, límite de tamaño y *pick*
  más reciente (`useStamps`), armado protegido (`useDrawing`), doble clic → una sola
  `stampPlacement`, *drop* fuera del borde recortado y *drop* de PDF sin sello
  (`DocumentViewer`).
- **No cubierto:** sin vista previa del sello antes del clic (el tamaño lo decide el core al
  colocar); un doble clic rápido no encola dos sellos (el segundo se rechaza, ver
  Colocación); sin pegar una imagen del portapapeles; un *drop* fuera de una página (en el hueco
  entre páginas) sigue yendo al workspace, que intenta abrir el PNG como PDF y falla con su
  mensaje; las imágenes de sesión viven en memoria como `data:` URL (una imagen grande pesa
  ~4/3 en memoria del shell, además de la copia del motor); no hay E2E con JPEG (solo PNG; la
  firma JPEG tiene Vitest); no se puede mover ni redimensionar un sello (T-240).
- **Para T-240…T-241:**
  - T-240: `stampImages` va por id; si mover/redimensionar/borrar+deshacer conservan el id
    de la anotación, la imagen sigue valiendo sin tocar nada (comprobarlo al implementarlo).
    El `<image>` llena la caja (`preserveAspectRatio="none"`): un redimensionado que no
    guarde la proporción la deformará en la capa.
  - T-241: sin cambios por los sellos.

### T-240 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Editing an annotation"). Sin cambios
en el core ni en `@vitela/core`. Lo que condiciona lo que sigue:

- **Seleccionar vive en Select (decisión).** Encima de la capa de texto de cada `PageSlot` hay
  una `AnnotationEditLayer` inerte al puntero salvo donde hay una anotación: un botón invisible
  por anotación (área = su caja; subrayado, tachado y tinta, 3 pt más por lado). Pulsar ahí
  selecciona (y puede arrastrar); pulsar fuera sigue seleccionando texto y quita la selección
  de la anotación. Solo hay una selección a la vez: texto o anotación. Solo se editan las de la
  sesión (las del archivo las pinta pdfium). Sin permiso de anotar, o con una herramienta armada,
  la capa no se monta.
- **Archivos nuevos en `features/annotations/`:** puros con tests: `editGeometry.ts` (caja,
  área, asas, límites, qué admite cada tipo), `editKeys.ts`, `editDialogs.ts` (lo tecleado),
  `editBuilds.ts` (cada edición como función de la anotación *actual*). Contenedores:
  `useAnnotationEditing.ts` (selección, diálogos, borrar), `useAnnotationDrag.ts` (el gesto y su
  vista previa), `useKeepFocus.ts`. Vistas: `AnnotationEditLayer`, `AnnotationToolbar`,
  `AnnotationDialog`, `LengthsDialog`, `FreeTextDialog` (sacado de `DraftDialog`) y `ModalForm`
  (el `<dialog>` modal, sacado de `TextDialog`). De `DocumentViewer` sale `useRevealMatch`
  (sin cambio de comportamiento). CSS en `styles/annotations.css`, bajo clases propias.
- **`useAnnotationEdits.change(id, pageKey, build, {grabbed, shownByCaller})`.** A diferencia
  de `add`, **espera su turno** en vez de rechazarse: una flecha repetida llega antes de que
  el core conteste. `build` recibe la anotación tal como la dejó el paso anterior, así que
  cada pulsación parte de donde acabó la otra; si devuelve `null` no se registra nada ni se
  marca sucio. Se descarta con aviso si la anotación ya no está, si su página cambió
  (`pageKey`) o —un arrastre, `grabbed`— si se movió desde que se agarró: nunca se aplica
  geometría vieja.
- **Arrastrar:** un paso al soltar; menos de 3 px de recorrido, o volver al punto de partida, no
  registra nada. La capa dibuja la anotación donde va el arrastre. Escape, `pointercancel` o
  captura perdida lo descartan, y también que desaparezca lo pulsado (el navegador avisa de
  esa captura al `document`, fuera de React) o que se suelte el botón primario. Otro arrastre
  antes de que aterrice el primero parte de donde este lo dejó. Ni mover ni redimensionar saca la caja de la página.
- **Redimensionar:** cuatro asas (la esquina opuesta queda quieta); el sello guarda la
  proporción con asas y teclado. La tinta no tiene asas ni "Resize…" (el core no la
  redimensiona); **la nota sí** (`resize_annotation` la admite). Suelo: 4 pt, y para texto libre
  el mínimo del core + 0,01 pt; un lado ya por debajo de 4 pt no crece si no se toca (la instantánea es `f32` y volvía una pizca por debajo).
- **Teclado:** Tab llega a cada anotación, Enter/Espacio la selecciona (o la suelta). Solo
  seleccionada y con el foco en ella (o en un asa): flechas 1 pt, Mayús 12 pt, Alt+flechas
  redimensionan, Supr/Retroceso borra, Escape suelta. Con el foco en un asa las flechas mueven
  esa esquina. Sin selección las flechas siguen desplazando. Los manejadores están en los
  elementos, no en `window`: el ámbito del editor, los diálogos y los campos de texto se
  respetan solos. Si lo enfocado desaparece (borrar, deshacer, herramienta armada, página
  desmontada), el foco pasa a la zona de páginas.
- **Barra "Annotation"** flotante abajo (aparecer en la columna de barras movería la página bajo
  el puntero): qué es y dónde (`aria-live`), Move to…, Resize…, Edit text… (texto libre),
  colores (marcado, forma, tinta) y Delete. Los diálogos usan puntos de la página dibujada
  desde arriba a la izquierda; "Keep proportions" viene marcado para sellos; un `EncodingGap`
  deja abierto el de texto. Al cerrarse devuelven el foco (también los de T-238, que lo
  perdían). Safari no enfoca un botón al pulsarlo: la barra lo enfoca a mano.
- **Selección e historia:** si un deshacer o un borrado se lleva la anotación, no queda nada
  seleccionado, y un rehacer no la vuelve a seleccionar.
- **Sellos:** mover, redimensionar y borrar+deshacer conservan el id (`ReplaceAnnotation` /
  `RemoveAnnotation`), así que `stampImages` sigue valiendo; comprobado en E2E.
- **Tests:** Vitest de los puros, 8 de `change` y 34 de `viewer/annotationEditing.test.tsx`
  (visor completo con un motor en memoria). Playwright (`e2e/edit-annotation.spec.ts`, 5 × 3
  navegadores): mover arrastrando, con flechas y con diálogo, redimensionar por asa, teclado y
  diálogo, recolorear, borrar, deshacer, y `/Rect` y `/C` de la descarga; teclado solo,
  desplazamiento y selección de texto; retexto con `EncodingGap` y `/Contents`; sello (proporción,
  imagen tras borrar+deshacer); página `/Rotate 90`. `annotate.spec.ts` comprueba además que el
  PDF sin permiso no ofrece nada. Los 7 primeros de `change` se vieron en rojo antes de implementar; los
  30 primeros del visor se escribieron antes, pero su rojo no distinguía (faltaba un proveedor en el
  arnés), así que se comprobaron después con seis mutaciones del código, todas detectadas. Los de la revisión se vieron en rojo. Los
  E2E se escribieron tras la UI.
- **Verificado en local (Windows):** wasm reconstruido (`npm run build:wasm`). `@vitela/core`:
  `typecheck`, Vitest 13 ficheros / 134, `build`. `@vitela/viewer`: `typecheck`, Vitest **63 /
  631**, `build`. `apps/web`: `typecheck`, Vitest 6 / 19, `build`; Playwright completo **163
  pasan, 20 omitidos** (los 148 / 20 de antes + 15). `edit-annotation`, `embed`, `draw`, `stamp` y `annotate` con `--repeat-each=2`: 126 / 126. `examples/embed-vite` (paquetes empaquetados): `setup`, `typecheck` y Playwright 12 / 12. Sin Rust tocado.
- **No cubierto:** sin prueba con pantalla táctil real ni con lector de pantalla; con el dedo,
  arrastrar una anotación **no** seleccionada desplaza o selecciona, pero no la mueve hasta
  que está seleccionada. La página girada tiene E2E solo para la forma a 90°; 180°/270°, texto
  libre y sello girados, sin E2E. Cada pulsación de flecha es un paso de deshacer (una tecla
  mantenida son muchos). Pulsar dentro de una forma grande la selecciona aunque solo se vea su
  borde, y el texto bajo una anotación no se puede empezar a seleccionar desde ella; de dos
  solapadas, el puntero toma la de encima (Tab llega a las dos). La barra flotante puede tapar
  una anotación pegada al borde inferior. Los cambios aún en cola al ir a Organize se descartan sin aviso (el que
  ya salió sí entra). Mover con el teclado no desplaza la vista para
  seguirla. Deshacer un borrado vuelve a poner la anotación la última en el orden de pintado
  (regla del core).
- **Para T-241:**
  - `useAnnotationEditing` tiene la selección: "Siguiente" puede seleccionar una anotación de la
    sesión con `input.select(id)`; las del archivo no tienen id y solo se pueden revelar. Para
    revelar una fuera de pantalla, `useRevealMatch` (búsqueda) es el patrón: caja desde el
    layout + `revealScroll`; el mismo gancho serviría para seguir a las flechas.
  - "Leer nota" puede abrir `TextDialog`/`ModalForm` en solo lectura; la barra "Annotation" es el
    sitio para el botón en una nota de sesión (hoy solo ofrece Move to, Resize y Delete).
  - El `<title>` de la nota sigue en el SVG inerte: no se ve al pasar el ratón.

### T-241 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Annotation navigation and
comments"). Sin cambios en el core ni en `@vitela/core`. Lo que condiciona lo que sigue:

- **Archivos (`features/comments/`):** puros con tests `stops.ts` (paradas, claves, paso) y
  `commentText.ts` (autor y fecha). Contenedores `useComments.ts` y
  `useAnnotationNavigation.ts`. Vistas `NavigationToolbar`, `CommentsPanel`, `NoteDialog` y
  `AnnotationRing`. Salen sin cambio de comportamiento `ModalShell` (de `ModalForm`),
  `focusing` (de `AnnotationToolbar`) y `useReveal` (de `useRevealMatch`). CSS en
  `styles/comments.css`.
- **Orden (decisión, distinto de los nativos):** Windows y Android recorren solo las
  anotaciones de la sesión, en orden de pintado. Aquí se recorren también los comentarios del
  archivo —si no, con anotar prohibido no habría nada que visitar—, así que el orden es por
  página y, en cada una, el de `comments`: los del archivo y luego los de la sesión en orden de
  pintado. Igual que en los nativos: da la vuelta por los dos extremos, y sin sitio previo
  entra por la primera o la última. Una anotación del archivo sin texto no se visita (el core
  no la lista).
- **El sitio se guarda por clave:** id en la sesión; `pageKey` + puesto en la página para las
  del archivo. Seleccionar a mano lo mueve; soltar la selección no lo pierde; si la anotación
  desaparece, se entra de nuevo.
- **Seleccionar o marcar:** la de la sesión se selecciona si se puede editar; si no (archivo,
  sin permiso, herramienta armada) se dibuja un aro que se va al pulsar una página o con
  Escape en ella. El foco se queda en el botón: pulsar otra vez sigue.
- **`useComments`:** vuelve a preguntar tras cada `info` nuevo (toda edición lo trae), una
  petición a la vez; las ediciones de mientras tanto cuestan una sola petición más y solo se
  muestra la respuesta a la última. La lista anterior se ve hasta entonces, salvo que cambien
  las páginas. Un fallo se muestra en el panel con "Try again".
- **Panel:** al lado de las páginas; debajo si el editor mide menos de 700 px (consulta de
  contenedor: vale también embebido). Escape lo cierra y devuelve el foco a su botón. Una fila
  va al comentario y "Read note" lo lee (en los nativos, pulsar hace las dos cosas).
- **La barra "Annotation" y lo revelado (medido):** a 390 × 600 con el panel abierto quedaban
  107 px de páginas y la barra, de dos filas (89 px), tapaba entera la nota de la sesión; a
  390 × 844 no. Ahora el salto se hace con la barra ya montada y descuenta lo que tapa; en un
  editor estrecho la barra es una sola fila (45 px) que se desplaza de lado, con el sitio solo
  para lectores de pantalla; y bajo la última página hay sitio fijo para subirla por encima.
  Medido después a 390 × 600: nota en 424–438, barra desde 456. El margen (80 px) es solo
  vertical y cede si no cabe; el de la búsqueda no cambia.
- **Sin saltos de página:** el texto del sitio ocupa siempre lo mismo (se corta con puntos
  suspensivos y va entero en el `title`). Un fallo de `comments` con el panel cerrado se marca
  en el botón, con un `status` para lectores de pantalla; abierto, solo avisa el panel.
- **De paso:** las herramientas de dibujo ya no desbordan a lo ancho en un móvil
  (`flex-wrap`; a 390 px la página medía 404).
- **Tests:** Vitest 45 nuevos: 38 en rojo antes de implementar (20 de los puros y
  `useComments`, 18 del visor), con seis mutaciones posteriores, todas detectadas, y 7 de la
  revisión, también vistos en rojo. Playwright (`e2e/comments.spec.ts`, 8 × 3): Siguiente y
  Anterior fuera de pantalla y en página girada 90°, panel con lectura y deshacer, página
  borrada en Organize, PDF sin permiso de anotar, editor estrecho, nota libre de la
  barra a 390 × 844 y 390 × 600, y páginas quietas al decir el sitio. `e2e/pdf.ts` gana `notes`.
- **Verificado en local (Windows):** `@vitela/viewer`: `typecheck`, Vitest 67 / 676, `build`.
  `apps/web`: `typecheck`, Vitest 6 / 19, `build`; Playwright completo 187 pasan, 20 omitidos
  (163 / 20 + 24); `comments` con `--repeat-each=3`: 72 / 72; `examples/embed-vite`: `setup`,
  `typecheck` y Playwright 12 / 12. Una pasada inicial dio un fallo de WebKit (el botón del diálogo de nota no llegó a estar "estable" en 30 s), no reproducido después. Vistas capturas anchas, oscuras y estrechas. Sin Rust tocado (wasm del
  24-11, igual al de `main`).
- **No cubierto:** sin lector de pantalla ni pantalla táctil reales; páginas giradas 180° y
  270° sin E2E. La lista no está virtualizada. El `<title>` de la nota sigue inerte: no se ve al
  pasar el ratón. El panel se cierra al ir a Organize y volver. Una nota
  de sesión se lee, pero no se puede retextear (regla del core).

### T-242 — resultado

Detalle de la API en [`packages/vitela-core/README.md`](../packages/vitela-core/README.md)
("Forms"). Ninguna regla nueva en el core: todo lo que hacía falta lo bajó T-235. Lo que
condiciona lo que sigue:

- **`pdf-wasm`** (`model/form.rs` + `model/form/tests.rs`, `engine/form.rs`,
  `dto/form.rs`, `convert/form.rs`): depende de `pdf-form`. `FieldFill { id, value:
  FieldValue }` en tipos del core. **Mismo orden que `pdf-ffi`:** `check_edit` con
  `EditClass::ANNOTATION` (bit 6; un test fija que es `EditClass::of` del comando
  construido) antes de mirar el id, luego `pdf_form::fill_field` (valida sobre una copia),
  luego el rechazo del modelo. `FormError::FieldNotFound` → `FormFieldNotFound` (tipo nuevo,
  el `FormFieldNotFound` de `pdf-ffi`); `UnsupportedOperation`/`InvalidValue` (tipo de valor
  equivocado, opción que no ofrece, texto más largo que `/MaxLen`) →
  `UnsupportedOperation` con las palabras del core, como `pdf-ffi`. `EditCommand` gana
  `setFieldText`, `setFieldChecked`, `setFieldChoice` (uno por tipo de valor; `None` = nada
  elegido); `Engine` gana `formFields` y `formFillAllowed`
  (`pdf_save::form_fill_allowed`).
- **Lista de campos (como `pdf-ffi`):** `/Fields` en orden, los del archivo incluidos (no
  está vacía al abrir), cada uno con la posición actual de su página; uno cuya página ya no
  está (borrada, guardada para deshacer) no se lista, igual que `list_form_fields`.
  Rellenarlo por id sigue llegando al core, que decide (no hay geometría que convertir).
  **Desviación:** `pdf-ffi` lista un `FormFieldKind` desconocido como texto de solo lectura
  (`max_len: Some(0)`); aquí se omite, como las anotaciones de T-236 (el lector del core solo
  produce los cuatro tipos, así que hoy no cambia nada). Tampoco se listaría un campo cuyo
  valor no casa con su tipo. El core no tiene banderas de solo lectura/obligatorio, así que
  no se exponen.
- **Forma del DTO (decisión).** Plano, como `AnnotationEntry`: una clase `FormFieldEntry`
  (`kind` `text`/`checkbox`/`radio`/`dropdown`, `name`, `rect`, `existing`, `multiline`,
  `maxLength`, `options`, `optionRects` —cuatro números por botón—, `editable`, `text`,
  `checked`, `selected`). `@vitela/core` lo decodifica (`worker/forms.ts`) a la unión
  `FormField` por `kind` (`text` con `value`; `checkbox` con `checked`; `radio` con
  `buttons: {exportValue, rect}[]` y `selected`; `dropdown` con `options`, `editable` y
  `selected`); un `kind` desconocido, un rect roto o botones que no emparejan se omiten; los
  ausentes pasan a `null`. Los rects van en la página dibujada (`PageFrame`), como las
  anotaciones: T-243 los usa tal cual para revelar el campo.
- **Refresco dentro de la petición (verificado).** Un relleno no es edición de anotación, así
  que `shows_in_session_preview` lo marca y `previewStale` queda en `true`: el `step` de
  `documentEngine` ya refresca en ese caso, sin cambios en su lógica. Probado en nativo
  (rellenar, deshacer y rehacer ponen la vista previa atrasada), con el núcleo falso
  (rellenar, deshacer y rehacer refrescan una vez cada uno; si el refresco falla se deshace el
  relleno) y contra pdfium real (el render siguiente ya pinta el valor nuevo en la caja del
  campo, y el deshacer lo devuelve a los mismos píxeles).
- **`@vitela/core`:** `Edit = PageEdit | AnnotationEdit | FormEdit` (`{ kind:
  'setFieldValue', fieldId, value: FieldValue }`, `FieldValue` = `text`/`checked`/`choice`).
  `EditedDocument` gana `formFields` (la lista, en cada respuesta: un relleno, un deshacer o
  un movimiento de página la cambian); `DocumentInfo` gana `formFillAllowed`; `Engine` gana
  `formFields`; protocolo `formFields`; constante `FORM_FIELD_NOT_FOUND`. `@vitela/viewer` y
  `apps/web`: solo fixtures de test retocados (los campos nuevos) y `formFields` en sus
  `fakeEngine`.
- **Acentos:** el texto "José Peña" (y uno multilínea con tildes) se rellena, guarda y
  reabre igual, en nativo y contra pdfium real.
- **Para T-243:**
  - Al abrir hay que pedir `formFields(handle)` (no viene en `OpenedDocument`); después,
    guardar `EditedDocument.formFields` en cada `revise`, como `annotations`.
  - Rellenar con el valor que el campo ya tiene **sí** registra un paso (como `pdf-ffi`; el
    core no lo trata como no-op): "confirmar al salir del campo" tiene que comparar antes de
    mandar.
  - `formFillAllowed` falso → panel de solo lectura; `EditNotPermitted` sigue siendo la
    respuesta del motor si se intenta.
  - Revelar un campo: `pageIndex` + `rect` (de un grupo de radio, la caja del grupo; cada
    botón tiene la suya en `buttons`). Todos los campos de hoy son `existing` (los pinta
    pdfium; la web no crea campos).
  - El fixture `reportlab_acroform.pdf` trae los cuatro tipos; `full_name` tiene `/MaxLen`
    100 (lo pone reportlab).
- **Verificado en local (Windows):** `cargo test --locked --no-fail-fast -p pdf-form -p
  pdf-save -p pdf-wasm` con pdfium vendorizado (534 pasados, 0 fallos, 7 ignorados;
  `pdf-wasm` 91 nativos, 12 nuevos); `cargo clippy --locked -p pdf-wasm --all-targets -- -D
  warnings` nativo; `cargo check --locked --target wasm32-unknown-unknown -p pdf-wasm` y
  `cargo clippy --locked --target wasm32-unknown-unknown -p pdf-wasm --all-targets -- -D
  warnings` (con `PDFIUM_WASM_DIR` apuntando a `apps/web/generated/pdfium`); `cargo fmt
  --all -- --check`. `@vitela/core`: `build:wasm`, `typecheck`, Vitest 146/146 (12 nuevos,
  incluido uno contra pdfium real: los cuatro tipos rellenados, pintado al instante,
  guardado, releído al reabrir, valor imposible e id desconocido con su error, deshacer que
  repinta). `@vitela/viewer`: `typecheck`, Vitest 67 / 676. `apps/web`: `typecheck`,
  Vitest 6 / 19.
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; los cuatro nuevos de
  `tests/forms.rs` —el criterio de hecho: rellenar texto/casilla/radio/lista, guardar,
  reabrir y leer los valores; valor imposible → `UnsupportedOperation`, id desconocido →
  `FormFieldNotFound`, sin permiso → `EditNotPermitted`, y el relleno que pinta tras
  `refreshPreview`— compilan con clippy wasm32 y los verifica el job `wasm32` de `core.yml`);
  Playwright y `build` de la app (sin UI nueva); `linux-gtk` (no toca).
- **No cubierto:** ninguna UI (T-243); cuadros de lista, botones y campos de firma no se
  listan; crear/mover/borrar campos (autoría) sigue fuera de la fase 3.

### T-243 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Forms"). Lo que condiciona lo que
sigue:

- **Archivos (`features/forms/`):** puro con tests `fieldValues.ts` (`sameValue`,
  `fieldsByPage`, `paintKeys`). Contenedores `useFormFields` (la lista), `useFormFill`
  (rellenar), `useDraft` (el texto escrito antes de confirmar) y `useForms`, que los junta con
  revelar y el aro. Vistas `FormsPanel`, `TextFieldInput` (texto y lista editable),
  `ChoiceInputs` (casilla, radio, lista fija) y `FormsToggle`; tipos en `fieldProps.ts`. CSS en
  `styles/forms.css`. Apoyo de test `src/test/formFields.ts` (un campo de cada tipo).
- **La lista:** `OpenDocument` gana `formFields` (lo guarda `revise` de cada
  `EditedDocument`); hasta la primera edición se pide `engine.formFields` una vez por
  documento (fallo → mensaje y "Try again" en el panel; al quitarse el aviso el foco pasa al panel con
  `useKeepFocus`, como en Comments).
- **Rellenar va por la cola del visor:** `useAnnotationEdits` gana `apply(build,
  shownByCaller)`, un paso genérico que recibe la sesión como la dejó el último paso (ahora
  con `formFields`); no mira el permiso de anotar, el panel mira `formFillAllowed`. Deshacer,
  rehacer y sucio no cambian. **Guardar espera a la cola:** `useAnnotationEdits` gana
  `settled()` y el visor se la da a `useSave` (`waitForEdits`, por
  `SaveWaitsForEditsContext`); tras preparar el destino y antes de pedir los bytes, el
  guardado espera a los pasos ya pedidos (un paso en cola no llega al engine hasta su turno, y
  sin esto un relleno hecho justo antes de Save faltaba en el archivo) y cuenta como guardados
  los que aterrizan en esa espera. **Un valor igual no se manda** (el core lo grabaría como
  paso): se compara al pedirlo contra el último valor pedido para ese campo que siga pendiente
  (si no hay, el que se ve) — así marcar y desmarcar deprisa, o volver al texto original antes
  de que aterrice el primer relleno, manda la vuelta atrás como relleno propio — y otra vez
  dentro del paso, contra la lista del último paso. `useDraft` hace lo mismo con el texto que
  se está mandando.
- **Confirmar (decisión):** texto y lista editable al salir del campo (Tab, clic fuera) o con
  Enter en una línea, sin perder el foco; en multilínea Enter es salto de línea. Casilla,
  radio y lista fija se confirman al cambiar (no hay nada que escribir); la elección se ve al
  momento y, al terminar el paso, lo que tenga el documento (vuelve atrás si el core la
  rechaza). Escape con texto cambiado lo devuelve al del documento sin cerrar; sin cambios,
  cierra el panel y el foco vuelve a "Forms". Un texto rechazado se queda en el campo para
  corregirlo.
- **Repintado:** pdfium pinta los campos en el bitmap y la página no cambia de `pageKey`, así
  que `usePageRender` gana `paintKey`: por página, los valores de sus campos; si cambia, se
  repinta conservando el bitmap anterior. La clave es `''` mientras los valores son los de la
  primera lista (la del bitmap ya pintado): listar no repinta nada, y solo se repintan las
  páginas cuyos campos cambian (también al deshacer).
- **Revelar:** al enfocar un campo del panel se lleva a la vista con `useReveal` (el margen de
  las anotaciones, con la barra "Annotation" como `foot`) y se marca con el aro de T-241
  (`AnnotationRing`; un botón de radio, con su propia caja); al salir se quita. Si la
  navegación tiene un aro, manda el suyo.
- **Paneles:** Comments y Forms comparten el lado de las páginas, uno a la vez. El botón
  "Forms" va en la fila de "Comments" (`children` de `NavigationToolbar`): con una barra
  propia un editor estrecho ganaba una fila y, medido, `comments` a 390 × 600 fallaba en los
  tres navegadores.
- **Solo lectura:** con `formFillAllowed` falso el panel lo dice, el texto es `readonly` (se
  puede enfocar y copiar) y casilla/radio/lista están `disabled`. Si el core contesta
  `EditNotPermitted`, el panel pasa a solo lectura sin aviso sobre las páginas.
  `UnsupportedOperation` y `FormFieldNotFound` se muestran bajo su campo (`role="alert"`,
  `aria-invalid`, `aria-describedby`); otros errores, en el aviso del visor.
- **Accesibilidad:** cada control con su `label` (el nombre completo `/T`), el radio como
  `radiogroup` con nombre, "(None)" en la lista fija (no elige nada), el orden de Tab es el de
  la lista. Textos en inglés: la web no tiene i18n.
- **Fallo del core encontrado y arreglado (`core/pdf-save/src/forms.rs`):** `ensure_acroform`
  solo reutilizaba un `/AcroForm` indirecto; uno en línea en el catálogo (el que escribe el
  propio `pdf-sign` y muchos productores) se cambiaba por uno nuevo con `/Fields []`, así que
  al guardar un relleno se perdían de `/Fields` todos los campos, la firma incluida. Ahora se
  pasa a objeto propio, sin cambios. Lo destapó el E2E del formulario firmado (al reabrir no
  había campos); test nuevo `ensure_acroform_keeps_an_inline_one`, visto en rojo. Afecta
  también a los nativos (`pdf-save` es común).
- **E2E (`e2e/forms.spec.ts`, 4 × 3):** los cuatro tipos del fixture de reportlab (Tab de
  `full_name` a `notes`, la página cambia de píxeles, guardar, reabrir y leer los valores);
  deshacer devuelve el valor y los mismos píxeles; formulario firmado → incremental con el
  prefijo abierto y el del fixture intactos, `/Fields` con la firma y el campo, y el valor al
  reabrir; PDF cifrado sin el bit 6 → solo lectura. `e2e/forms.ts` construye el firmado en
  memoria (una revisión más sobre `signed/rsa2048_sha256.pdf` con un campo de texto);
  `e2e/pdf.ts` gana `fields` (campos de texto).
- **Tests:** Vitest 44 nuevos en `@vitela/viewer` (puros, hooks, panel, visor, cola,
  `revise`, `paintKey`), vistos en rojo antes de implementar salvo los de `useFormFields`
  (escritos junto con el hook).
- **Verificado en local (Windows):** `@vitela/viewer`: `typecheck`, Vitest 72 / 720.
  `apps/web`: `typecheck` (incluye el `build` de los paquetes), Vitest 6 / 19, `build`;
  Playwright completo 199 pasan, 20 omitidos; `forms`, `comments` 390 × 600 y `draw` con
  `--repeat-each=3`: 63 / 63. `@vitela/core`: `build:wasm` (con el arreglo), Vitest 14 / 151.
  Rust: `cargo test --locked -p pdf-save -p pdf-sign -p pdf-form` (501, 0 fallos, 7
  ignorados), `-p pdf-wasm` (91), `-p pdf-ffi` (369); `cargo clippy --locked -p pdf-save
  --all-targets -- -D warnings`; `cargo fmt --all -- --check`. Capturas ancha, estrecha
  (390 × 844) y oscura vistas.
- **No corrido en local:** `wasm-bindgen-test` (sin chromedriver), `linux-gtk`, Android,
  `examples/embed-vite`.
- **No cubierto:** cuadros de lista, botones y campos de firma no se listan (T-242); crear,
  mover o borrar campos (autoría) sigue fuera de la fase 3; pulsar un campo en la página no
  lo enfoca en el panel (solo panel → página); el panel se cierra al ir a Organize y volver;
  sin lector de pantalla ni pantalla táctil reales; la lista no está virtualizada.
  Visto de paso, sin tocar: en `draw.spec`, "Add text box" con un carácter fuera de WinAnsi
  tiene una carrera latente (si la respuesta del core llega después de reescribir el texto,
  el error vuelve). Con la barra propia de Forms falló 2 de 12; con el botón en la fila de
  Comments, 0 de 9.

## Fase E — Visor embebible (`@vitela/viewer`)

Decisión del usuario (2026-11-21): Vitela se tiene que poder instalar en **cualquier web**
con `npm install`, **con la interfaz**, no solo el motor. El objetivo final es editar
textos desde cualquier web; eso necesita antes la edición de contenido en wasm (Fase 4,
`pdf-wasm` hoy no depende de `pdf-edit`), así que esta fase empaqueta lo que la web ya
hace (ver, organizar, importar, anotar, sellos) y la Fase 4 llegará al paquete sin tocar la
integración.

Hechos de partida (mapa de `apps/web`, 2026-11-21):

- `features/document/DocumentWorkspace.tsx` ya es una raíz limpia (`engine`, `intro`,
  `initialScreen`, `standIn`); el resto de `app/`, `features/tools/` y la landing son la
  *app*, no el editor.
- Acoples que impiden embeber: tokens en `:root`, reglas sobre `body`/`a`/`button` sin
  clase, `.app` a `100vh`, `.drop-hint` `position: fixed`, clases sin prefijo; soltar
  archivos escucha en `window` (`useFileDrop`); Ctrl+Z/Y, copiar y Escape escuchan en
  `window` (`useUndoKeys`, `useCopy`, `useDrawing`); `modalOpen()` mira todo el
  `document`; `download.ts` cuelga el `<a>` de `document.body` (inofensivo).
- No hay forma de abrir desde bytes/URL ni de recibir el guardado: `Delivery`
  (`save/delivery.ts`) ya es la costura, pero `DocumentWorkspace` no la expone.
- Ni `@vitela/core` ni nada está publicado en npm (`npm view @vitela/core` → 404).

| Tarea | Entrega | Verificación |
|---|---|---|
| **T-244** ✓ | `packages/vitela-viewer` (`@vitela/viewer`, workspace npm): se **mueve** ahí la UI del editor (`DocumentWorkspace` y `features/document`, `viewer`, `organize`, `annotations`, `render`, `search`, `text`, `save`, `platform`, las piezas de `ui/` que usa, `useEngineStatus`, sus estilos y sus tests). `apps/web` lo consume como dependencia. **Cero cambio de comportamiento.** | Vitest del paquete y de la app en verde (mismos totales repartidos); Playwright completo de la app en los tres navegadores; `typecheck`; build. |
| **T-245** ✓ | CSS acotado: tokens y reset bajo la clase raíz `.vitela` (nada en `:root`, `body` ni selectores sueltos), el editor ocupa su contenedor (`height: 100%`, no `100vh`), `.drop-hint` relativo a la raíz. La app sigue igual. | Playwright: página anfitriona de prueba con estilos agresivos propios (`button`, `body`, fuente) — los del editor no salen y los de la anfitriona no rompen el editor en lo medible. |
| **T-246** ✓ | Eventos acotados: soltar archivos solo sobre el editor (opción para toda la ventana, que usa la app), atajos solo con el foco dentro del editor, `modalOpen()` mira solo los diálogos del editor. | Vitest de cada hook; Playwright en la anfitriona: soltar fuera no abre, Ctrl+Z fuera no deshace. |
| **T-247** ✓ | API pública: componente React `<VitelaViewer>` (`source`: `File`/`Uint8Array`/URL; `engine` o `engineOptions`; `onSave(bytes, name)` como `Delivery`; ocultar el selector; pantalla inicial) y `mountViewer(element, options)` sin React para quien no lo use (`destroy`, `open`, `save`). React como `peerDependency`. README con el `vite.config` que necesita un consumidor (wasm/worker). | Vitest de la API; Playwright sobre un ejemplo consumidor (`examples/embed-vite`) que instala el paquete empaquetado (`npm pack`), abre desde URL, edita y recibe los bytes en `onSave`. |
| **T-248** | Publicación: workflow por tag que publica `@vitela/core` y `@vitela/viewer` con provenance, solo tras `web.yml` en verde. Workflow hecho, primera publicación pendiente (ver "T-248 — resultado"). **Pasos del usuario:** crear la organización `vitela` en npmjs.com y configurar la publicación de confianza (OIDC) de ambos paquetes para el workflow. | Ensayo con `npm publish --dry-run` en CI; primera versión publicada e instalada desde el registro en el ejemplo consumidor. |

Riesgos abiertos (NO verificados):

- Vite pre-empaqueta las dependencias (`optimizeDeps`) y eso rompe
  `new Worker(new URL('…', import.meta.url))` dentro de `node_modules`; probablemente el
  consumidor necesite `optimizeDeps.exclude: ['@vitela/core']`. Se comprueba en T-247.
  **Comprobado en T-247:** con Vite 8.3.8 no hace falta (ver "T-247 — resultado").
- Estilos de la anfitriona que se cuelan **dentro** del editor: la clase raíz evita que
  los nuestros salgan, no que entren los suyos. Shadow DOM lo resolvería pero choca con
  los diálogos y la selección de texto; se decide con la anfitriona de prueba de T-245.

### T-244 — resultado

Mudanza pura con `git mv` (el historial sigue a los ficheros); ninguna regla CSS ni
línea de lógica cambia.

- **Se movió a `packages/vitela-viewer/src/`:** `features/` (`document`, `viewer`,
  `organize`, `annotations`, `render`, `search`, `text`, `save`), `platform/`,
  `ui/` (`FilePicker` y su test, `StatusLine`, `ViewSwitch`), `engine/useEngineStatus.ts`
  (antes `app/`), `test/fakeEngine.ts` y `styles/` (`tokens`, `base`, `viewer`,
  `organize`, `dialogs`, más un `index.css` nuevo que los importa en ese orden). Las
  rutas relativas internas no cambian porque `src/` conserva la forma de la app.
- **Se queda en `apps/web`:** `main.tsx`, `app/App.tsx` (+ test), `features/tools/`, las
  piezas de la landing en `ui/` y `landing.css` + `tools.css` (`app/styles/index.css`
  ya solo importa esos dos). `main.tsx` importa `@vitela/viewer/style.css` **antes**
  que `index.css`, así que el orden de la cascada es el de siempre: el CSS del build es
  **idéntico byte a byte** (mismo hash `index-DVwe0LQM.css`, `cmp` sin diferencias).
- **API pública mínima** (`src/index.ts`): `DocumentWorkspace` + `DocumentScreen`,
  `useEngineStatus` + `EngineStatus` y `StatusLine` — exactamente lo que `App.tsx` y
  `tools.ts` usan. Exports: `.` (`dist/index.js` + `.d.ts`) y `./style.css`
  (`dist/styles/index.css`). React y React DOM 19.3.1 son `peerDependencies`;
  `@vitela/core` 0.1.0 es `dependency`.
- **`dist`, no alias de Vite.** La app consume el `dist/` del paquete igual que ya hace
  con `@vitela/core`: lo que se construye y se prueba (Vitest de la app, build, los tres
  navegadores de Playwright) es lo que instalará un consumidor, y no hay `paths`/alias
  que solo existan en este repo. El coste es un paso de build: los scripts de la app
  corren `build:packages` (core y luego viewer) y `scripts/dev.mjs` arranca también el
  `watch.mjs` del paquete (tsc `--watch` + copia de los CSS). En dev, viewer y app
  resuelven `@vitela/core` al mismo `dist/index.js` y React al mismo pre-bundle (una
  sola instancia de cada uno).
- `App.test.tsx` necesita `fakeEngine`, y el paquete no publica helpers de test: la app
  guarda una **copia** de 29 líneas en `src/test/fakeEngine.ts`. Si `Engine` crece, el
  typecheck de ambos lados avisa.
- CI: `web.yml` y `web-deploy.yml` corren typecheck, Vitest y build de
  `@vitela/viewer` tras los de `@vitela/core` (sin eso, `web-deploy` habría dejado de
  correr 481 tests unitarios antes de publicar).

Gates (Windows 11, local):

- Vitest antes: **58 ficheros / 499 tests** en un worktree limpio de `HEAD` (d678ea3).
  Después: `@vitela/viewer` 52 / 481 + `apps/web` 6 / 18 = **58 / 499**, misma lista
  de ficheros. Ojo: una primera pasada en el checkout principal antes de mover nada dio
  59 / 517 (un fichero de 18 tests de más) y no se pudo reproducir ni identificar; el
  worktree de `HEAD` sí cuadra fichero a fichero.
- `npm run typecheck`, `npm test` y `npm run build` en `packages/vitela-viewer` y en
  `apps/web`: verde. `npm pack --dry-run` del viewer: `dist/` (sin tests) + licencias.
- Playwright de `apps/web` en Chromium, Firefox y WebKit: **115 pasan, 20 omitidos**,
  igual que antes de mover.
- Ningún fichero de `apps/web` importa por ruta relativa un módulo movido (grep).

Lo que condiciona T-245…T-247:

- T-245 trabaja ya sobre `packages/vitela-viewer/src/styles/`; `tokens.css` y
  `base.css` (`:root`, `body`, `a`, `button`) siguen globales y **la landing de la app
  depende de esos tokens**: al acotarlos bajo `.vitela`, la app tiene que seguir
  recibiéndolos (p. ej. la app pone `.vitela` en su raíz o importa los tokens aparte).
- El JSX del paquete lo compila `tsc` (`react-jsx`), no `@vitejs/plugin-react`: en
  dev, los avisos de React desde el editor pierden la ubicación de fuente (runtime
  `jsx`, no `jsxDEV`) y no hay Fast Refresh dentro del paquete (recarga vía el watch).
- T-247 decide la API real; `StatusLine` y `useEngineStatus` están exportados solo
  porque la app los usa y pueden dejar de ser públicos. El riesgo de `optimizeDeps` sigue
  sin verificar: los workspaces enlazados no se pre-empaquetan, un consumidor desde npm sí.

### T-245 — resultado

El CSS de `@vitela/viewer` ya no sale de su raíz: tokens y reset viven bajo `.vitela`, que
`DocumentWorkspace` lleva en su elemento raíz (`<div class="vitela workspace">`).

- **Tokens** (`tokens.css`): `:root` → `.vitela`, también dentro de
  `@media (prefers-color-scheme: dark)`. La raíz fija además `color: var(--text)`,
  `background: var(--surface)`, `color-scheme` y la fuente, así que lo que hereda la
  anfitriona (`body { font-family; color }`) no llega al texto del editor.
- **Reset** (`base.css`): `body` y `.app` salen del paquete (son de la app); `a` y
  `button` pasan a `:where(.vitela) a` / `:where(.vitela) button…`. `:where()` no suma
  especificidad: el reset pesa lo mismo que el `button` suelto de antes y toda regla de
  clase (`.button--primary`, …) lo sigue ganando. Con `.vitela button` (0,1,1) la regla
  habría pisado a `.button--primary` (0,1,0).
- **Tamaño:** `.workspace` lleva `height: 100%`, `box-sizing: border-box` y
  `position: relative`. Como elemento flex (la app) manda `flex: 1` y el `height` no
  cuenta; en un bloque con altura (la anfitriona) llena el contenedor.
- **`.drop-hint`:** `position: absolute` contra la raíz (antes `fixed` al viewport).
- **Sin `rem`:** los 10 `font-size` en `rem` pasan a `calc(N * var(--font-size))`, con
  `--font-size: 15px` en `.vitela`. `rem` es el tamaño raíz **de la anfitriona**; en la
  app daba 15px porque los tokens ponían la fuente en `:root`, así que el valor es el mismo.
- **La app:** pone `class="vitela"` en `<html>` (`index.html`). Es lo más simple que la deja
  igual: el `SiteHeader`, la landing, `ToolUnavailable` y el propio `<html>` siguen dentro
  del ámbito, con el `color-scheme` y la fuente en el elemento raíz (barras de scroll,
  lienzo, `rem` de `landing.css`). Ponerla en `main.app` habría dejado fuera `<html>` y
  `body`; importar los tokens aparte obligaba a duplicarlos. `body { margin; background;
  color }` y `.app` (100vh) pasan a `apps/web/src/app/styles/base.css`.
- **Diálogos:** son `<dialog>` hijos del árbol de la raíz; en la capa superior siguen
  heredando las propiedades personalizadas por el DOM, así que reciben los tokens. Los
  `::backdrop` usan colores literales. No hay portales (`createPortal`) en el paquete.

Anfitriona de prueba: `apps/web/e2e/host/` (HTML + `main.ts` + `host.css`), con su propio
build de Vite a `e2e/host/dist/` (ignorado por git) y servida por un segundo `webServer` de
Playwright en el puerto 4174 (`npm run preview:host`), así nunca entra en el `dist/` que se
despliega. Estilos agresivos de la anfitriona, cargados **después** de los del editor:
`:root { --accent; --text; font-size: 20px }`, `body` (margen, fondo, color, Courier,
22px, `line-height: 2`), `a`, `button` (fondo, borde, radio, padding, Courier,
`text-transform`, `letter-spacing`).

Lo que se mide (`e2e/embed.spec.ts`, 6 tests × 3 navegadores):

- No sale nada: `html` y `body` sin `--surface`/`--viewer-bg`, `color-scheme: normal`,
  `font-size` raíz 20px; `body`, los botones de la anfitriona (antes y después del editor)
  y su enlace conservan exactamente sus estilos.
- El editor ocupa su contenedor (640px), también con un documento abierto, y el scroller
  no se sale; el aviso de soltar cae dentro de la raíz con 12px de margen y mide 19,5px
  (15 × 1,3), no 26px.
- No entra lo heredado: dentro de la raíz `--accent`/`--text` son los nuestros pese al
  `:root` de la anfitriona; color, fondo, fuente, 15px/22,5px y `color-scheme` también; un
  documento abre y pinta. El diálogo de contraseña (capa superior) tiene fondo, color,
  borde y fuente del editor y el título mide 17,25px (15 × 1,15).
- **Fuga conocida, fijada en un test:** los selectores de tipo de la anfitriona **sí**
  llegan a los elementos del editor. Las propiedades que el editor no fija pasan tal cual
  (`text-transform: uppercase`, `letter-spacing: 3px` en los botones de `.view-switch`), y
  en las que ambos fijan con la misma especificidad gana la hoja que carga después (aquí
  la anfitriona: la fuente Courier en esos botones). Lo mismo vale para `p`, `input`,
  `h2`, etc. y para nombres de clase sin prefijo que coincidan (`.dialog`,
  `.button--primary`, `.workspace`, …): las reglas de clase del editor no están bajo
  `.vitela` y saldrían a un elemento de la anfitriona con esa clase. Decisión: **sin
  Shadow DOM** (choca con diálogos y selección de texto); se documenta y se mide.

Que la app se ve igual: el CSS del build ya no es idéntico byte a byte (cambian los
selectores), así que se compararon los estilos computados de **cada elemento** (todas las
propiedades + caja) entre el build de `HEAD` y el nuevo, en claro/oscuro y 1280/390px, en
inicio, `/organize`, `/sign`, visor con documento, Organize con documento, diálogo de
contraseña y aviso de soltar (script desechable, no versionado). Chromium y Firefox: los
28 escenarios cada uno; WebKit: 6 (el escenario de soltar agotó el tiempo en el script y
el resto no corrió; los flujos de WebKit sí pasan en Playwright). Única diferencia
visible: el aviso de soltar empieza bajo la cabecera del sitio (antes cubría el viewport).
El resto son diferencias sin efecto en la caja: `<html>` con color/fondo propios (el
`body` sigue pintando lo mismo), la raíz del editor con clase, `position: relative`,
`box-sizing` (el `width` computado cambia de 1248 a 1280 por el modelo de caja, la caja es
idéntica) y fondo `--surface` sobre el mismo `--surface`; y el `top` resuelto de spans
absolutos ocultos, con la misma caja.

Gates (Windows 11, local):

- `@vitela/viewer`: `tsc -b` y build en verde; Vitest **53 ficheros / 487 tests**
  (antes 52 / 481: `styles/scoped.test.ts` con 5 tests que leen las hojas y fallan con
  `:root`, selectores que no empiezan por clase, `.vitela ` sin `:where()`, `100vh`,
  `position: fixed` o `rem`; y uno en `DocumentWorkspace.test.tsx` para la clase raíz).
  Todos se vieron en rojo antes del cambio.
- `apps/web`: `npm run typecheck`, `npm test` (**6 / 18**) y `npm run build` en verde.
- Playwright de `apps/web` (Chromium, Firefox, WebKit): **133 pasan, 20 omitidos** (los
  115 / 20 de T-244 + 18 = 6 × 3 de `embed.spec.ts`). Una primera pasada completa falló en
  Firefox porque la caja medía 640,00002px; las cajas se comparan ahora con tolerancia
  subpíxel, y `embed.spec.ts` pasó 54/54 con `--repeat-each=3`. Los 6 de
  `embed.spec.ts` se vieron en rojo antes del cambio (5 fallaban en Chromium; el de la
  fuga conocida ya pasaba).

Lo que condiciona T-246/T-247:

- T-246: el aviso de soltar ya se dibuja dentro de la raíz, pero `useFileDrop` sigue
  escuchando en `window`; en la anfitriona, soltar fuera del editor aún lo muestra y abre.
  La anfitriona de `e2e/host/` es donde van sus tests (soltar fuera, Ctrl+Z fuera).
- T-247: `<VitelaViewer>` hereda la clase raíz de `DocumentWorkspace`; el consumidor solo
  tiene que importar `@vitela/viewer/style.css` y darle altura al contenedor. El README
  debe decir que el editor llena su contenedor y avisar de la fuga de selectores de tipo
  (y de las clases sin prefijo) si no se resuelve antes. La media query `max-width: 600px`
  del visor sigue mirando el viewport, no el contenedor (container queries, si hace falta).
- La landing de la app sigue leyendo tokens que solo usa ella (`--tone-*`, `--card-shadow`,
  `--illo-*`) y que viven en el paquete; moverlos a la app es limpieza aparte.

### T-246 — resultado

Los eventos de página del editor ya no salen de su raíz. Una pieza nueva,
`features/document/editorScope.ts`, decide qué evento es del editor; `DocumentWorkspace`
la crea sobre su elemento raíz y la pasa a los hooks (por parámetro a `useFileDrop`; por
`EditorScopeContext` al visor y a Organize, que la pasan a `useUndoKeys`, `useCopy`,
`useDrawing` e `historyAction`). Sin proveedor, el ámbito es `WHOLE_DOCUMENT` (todo el
documento, el comportamiento de antes), así que un hook suelto sigue igual.

- **Una sola opción, `eventScope?: 'editor' | 'window'`** en `DocumentWorkspace` (tipo
  `EventScope`, exportado desde `src/index.ts`), por defecto `'editor'`. Gobierna a la vez
  soltar archivos y teclas (Ctrl/⌘+Z/Y, Ctrl/⌘+C, Escape). Con `'window'` el ámbito acepta
  todo evento de la ventana, como antes de T-246; con `'editor'`, solo los del editor. En
  los dos modos, `modalOpen()` mira solo los diálogos del editor. Una primera versión
  tenía solo `dropTarget` y acotaba las teclas también en la app; la revisión lo descartó:
  en la app, un sello soltado sin haber hecho clic antes no se deshacía con Ctrl+Z (soltar
  no es `pointerdown` ni `focusin`), ni tras un clic en la cabecera del sitio.
- **`EditorScope`** es un objeto estable con tres preguntas: `ownsEvent(target)` (¿una tecla
  dirigida ahí es del editor?), `takesDrop(target)` (¿un archivo arrastrado ahí?) y
  `modalOpen()` (¿hay un diálogo del editor abierto?). Lee la raíz, el modo y la última
  interacción al preguntarle; ningún listener se vuelve a registrar.
- **"El foco está dentro"** (modo `'editor'`): la tecla va a un elemento dentro de la raíz,
  **o** va a `<body>` (nada tiene el foco) y la última interacción del usuario cayó dentro de
  la raíz: `pointerdown`, `focusin` o `drop`, escuchados en captura en `window`. Cubre los
  casos en que el foco queda en `body` aunque el usuario esté en el editor: un clic en la
  zona de páginas (no enfocable), un control enfocado que desaparece (un botón que se
  deshabilita) y soltar un archivo o un sello sin haber hecho clic. Se descartó
  `tabIndex=-1` en la raíz: un clic la enfocaría y habría que esconder su contorno, y no
  resuelve ni el control que desaparece ni la caída.
- **Soltar archivos** (modo `'editor'`): solo cuentan los arrastres cuyo destino está
  dentro de la raíz; fuera no hay aviso, no se abre nada y **no** se llama a
  `preventDefault` (lo que el navegador haga con un archivo soltado sobre la anfitriona es
  cosa de la anfitriona). Los listeners siguen en `window` en los dos modos: Organize
  reclama la caída con un `onDrop` de React, y React escucha en su contenedor raíz, **por
  encima** de la raíz del editor; un listener nativo en la raíz lo vería antes que el
  reclamo y abriría el PDF en vez de añadirlo. El conteo `dragenter`/`dragleave` solo
  cuenta destinos dentro (al salir hacia la anfitriona, `dragenter` fuera llega antes que
  `dragleave` dentro, y el conteo vuelve a 0), salvo un `dragleave` cuyo destino ya no
  está en el documento (`!isConnected`): ese cuenta siempre, o el aviso se quedaría
  puesto. Límite: un elemento quitado **antes** de que salga su `dragleave` no lo hace
  llegar a `window` (no tiene camino hasta allí), igual que antes de T-246.
- **Teclas:** `useUndoKeys`, `useCopy` y `useDrawing` piden `ownsEvent` antes que nada.
  Organize ya escuchaba en su `<section>` (`onKeyDown`), así que solo cambia su chequeo de
  diálogos. No hay más listeners de teclado o portapapeles en `window`/`document` en el
  paquete (grep).
- **`modalOpen()`** mira solo `dialog[open], [aria-modal="true"]` **dentro de la raíz**
  (los diálogos del editor son hijos de su árbol aunque se pinten en la capa superior). El
  `modalOpen(target)` de `historyKeys.ts` desaparece; `historyAction` recibe el ámbito como
  tercer parámetro (por defecto, todo el documento).
- **La app** pasa `eventScope="window"` (`App.tsx`) y se comporta igual que antes: soltar
  en toda la ventana (el `dropFile` de sus E2E dispara sobre `main.app`, **fuera** de la
  raíz) y teclas desde cualquier sitio. La única diferencia posible es de diálogos: un
  diálogo abierto fuera del editor ya no frena sus teclas, y la app no tiene ninguno.
  Ningún test Playwright existente tuvo que cambiar.

Tests nuevos de punta a punta:

- App (`e2e/keys.spec.ts`, 2): soltar un PDF y luego un PNG sobre la página, sin ningún clic,
  y Ctrl+Z quita el sello; lo mismo tras un clic en la cabecera del sitio.
- Anfitriona (`e2e/embed.spec.ts`, 3; sin elementos nuevos, bastan `#host-button` y
  `body`): soltar sobre `#host-button` y sobre `body` no toma ningún evento, no muestra
  aviso ni abre nada, y sobre `.vitela` sí; con el foco en `#host-button`, Ctrl+Z y Escape
  no hacen nada, y tras un clic en la línea de estado del editor (foco en `body`) sí;
  soltar un PDF y un sello sobre el editor sin hacer clic y Ctrl+Z quita el sello.

Gates (Windows 11, local):

- `@vitela/viewer`: `npm run typecheck`, `npm test` y `npm run build` en verde. Vitest
  **54 ficheros / 505 tests** (antes 53 / 487): `editorScope.test.tsx` nuevo (9) y 9 más en
  `historyKeys` (1), `useUndoKeys` (2), `useCopy` (1), `useDrawing` (1), `useFileDrop` (2)
  y `DocumentWorkspace` (2). Todos se vieron en rojo antes de implementar, salvo dos que
  fijan lo que ya era así: `DocumentWorkspace` con `eventScope="window"` y, añadido en la
  revisión, "con `'window'` solo cuentan los diálogos propios".
- `apps/web`: `npm run typecheck`, `npm test` (**6 / 19**, antes 6 / 18: soltar sobre la
  cabecera del sitio abre el PDF; rojo sin la opción) y `npm run build` en verde.
- Playwright de `apps/web` (Chromium, Firefox, WebKit): **148 pasan, 20 omitidos** (los 133 / 20 de
  T-245 + 15 = 5 × 3). Los 5 nuevos se vieron en rojo en Chromium: los 2 primeros de la
  anfitriona contra el código previo a T-246, y los 3 de "sin clic"/cabecera contra la
  primera versión de T-246 (el sello seguía tras Ctrl+Z).

Lo que condiciona T-247:

- `<VitelaViewer>` y `mountViewer` deben exponer `eventScope` (por defecto `'editor'`). El
  README tiene que contar que, embebido, las teclas solo actúan con el foco (o el último
  clic o caída) dentro del editor, y que fuera de él soltar un archivo es cosa de la
  anfitriona (si no lo trata, el navegador lo abre).
- `EditorScopeContext` y el resto de `editorScope.ts` no se exportan; solo el tipo
  `EventScope`.
- Shadow DOM: `contains` y `isConnected` no cruzan sombras; si T-247 monta el editor en un
  shadow root, el ámbito hay que revisarlo allí.
- Dos editores en la misma página: cada uno tiene su raíz y su ámbito, pero no se probó.

### T-247 — resultado

`@vitela/viewer` tiene ya una API para cualquier web: `<VitelaViewer>` (React) y
`mountViewer(element, options)` (sin React propio), en `packages/vitela-viewer/src/embed/`.
`<VitelaViewer>` es un contenedor sobre `DocumentWorkspace`: arranca (y para) su motor si no
le dan uno, abre lo que le pasa la anfitriona, manda los guardados a `onSave` y dice cuándo
el motor carga o falla. El marcado sigue siendo el del workspace.

- **Superficie pública** (`src/index.ts`): `VitelaViewer` + `VitelaViewerProps`,
  `VitelaViewerHandle`, `ViewerEngine`; `mountViewer` + `MountViewerOptions`,
  `MountedViewer`; los tipos `ViewerSource` y `SaveCallback`. Siguen públicos
  `DocumentWorkspace` + `DocumentScreen`, `EventScope`, `useEngineStatus` + `EngineStatus` y
  `StatusLine`: la app (`App.tsx`, `tools.ts`) y su anfitriona de prueba (`e2e/host/main.ts`)
  los usan para montar su propio shell (landing y rutas de herramientas alrededor del
  editor), así que quitarlos rompería `apps/web`. El README los presenta como piezas de bajo
  nivel. `EditorScopeContext`, `Delivery` y `callbackDelivery` no se exportan.
- **Props** (y opciones de `mountViewer`, las mismas sin `ref`): `source` (`File`,
  `Uint8Array` o URL `string`/`URL`), `name`, `engine` **o** `engineOptions`, `onSave(bytes,
  name)`, `filePicker` (por defecto `true`), `initialScreen` y `eventScope` (por defecto
  `'editor'`, como pidió T-246). Un `source` nuevo abre el documento nuevo.
- **Fuentes:** `useDocument` ya no pide un `File` sino un `PdfSource` (`name` +
  `arrayBuffer()`), que un `File` cumple tal cual, así que el selector y la caída no cambian.
  Los bytes se **copian** en cada lectura (el motor los recibe por transferencia y el array de
  la anfitriona quedaría separado); una URL se pide con `fetch` **una vez** y se reutiliza en
  un reintento de contraseña (un fallo sí se reintenta). Un fallo al leer es un fallo al
  abrir, con su mensaje en la línea de estado: `Could not open x.pdf: HTTP 404 Not Found`. El
  nombre sale de `name`, si no del último tramo de la ruta de la URL (decodificado), si no
  `document.pdf`.
- **`onSave` como `Delivery`:** `callbackDelivery` (`features/save/callbackDelivery.ts`)
  resuelve `prepare` al momento (no hay fichero que comprobar ni permiso que pedir) y
  `deliver` espera al callback. Un resultado nuevo, `{ how: 'delivered' }`, quita la marca
  de "cambios sin guardar" igual que `wrote` (la anfitriona ya tiene las ediciones) y se
  lee "Saved x.pdf."; si el callback rechaza, el guardado falla con su mensaje. El callback
  más reciente se lee por `ref`, así que una función nueva en cada render no rehace la
  entrega. Sin `onSave`, la entrega es la de siempre (`browserDelivery`).
- **`engine` o `engineOptions`:** con `engine`, la anfitriona lo mantiene vivo tras
  desmontar. Sin él, el visor llama a `createEngine(engineOptions)` en un efecto (nunca en el
  render: ni SSR ni dobles workers en StrictMode) y lo para al desmontar. Para eso
  `@vitela/core` gana `EngineClient.terminate()`: hace fallar lo pendiente y lo posterior
  con "the engine was terminated", también un `ready` pendiente, y llama a
  `Worker.terminate()` (`MessagePortLike.terminate?` es opcional). Sin `terminate`,
  `destroy()` habría dejado un worker con pdfium vivo por cada montaje. Un `createEngine`
  que lanza (opción inválida) se muestra como "The PDF engine failed to load: …".
- **Al desmontar,** `useDocument` cierra en el motor el documento en pantalla. Con un motor
  compartido, cada montaje habría dejado su documento abierto en el worker.
- **Handle:** `open(source, name?)` y `save()` (más `destroy()` en `mountViewer`) **no
  devuelven nada**. Cómo fue la apertura o el guardado lo dice la línea de estado del editor,
  y los bytes llegan a `onSave`. Una promesa habría tenido que decidir qué significa
  "terminado" con una contraseña pendiente o una pregunta de firma abierta, y habría
  cambiado el tipo de `open`/`save` en hooks que usan decenas de tests. `open` antes de que
  el motor esté listo espera; la última petición gana y cada una se abre una sola vez (un
  `ref` con su número, por los efectos dobles de StrictMode). `DocumentWorkspace` gana
  `filePicker`, `delivery` y un `ref` (`WorkspaceHandle`) con esas dos operaciones.
- **`mountViewer`** usa `createRoot` y un `flushSync` en el primer render, así que el handle
  funciona en cuanto vuelve. `destroy()` desmonta y vacía el elemento; las llamadas
  posteriores no hacen nada, y destruir dos veces no falla.
- Sin documento abierto, el visor centra la acción de abrir (o el estado del motor) en
  `.workspace__empty` (regla nueva en `viewer.css`, dentro de lo que exige
  `styles/scoped.test.ts`).

**El riesgo de `optimizeDeps`, comprobado** (Vite 8.3.8, paquetes empaquetados, Chromium,
Firefox y WebKit): **no hace falta `optimizeDeps.exclude`**. En dev, Vite pre-empaqueta
`@vitela/viewer` **junto con** `@vitela/core` en `node_modules/.vite/deps/@vitela_viewer.js`
(visto en `_metadata.json`) y reescribe el
`new Worker(new URL('./worker/worker.js', import.meta.url))` a
`new URL("../../@vitela/core/dist/worker/worker.js", import.meta.url)`, que apunta otra vez
al fichero del paquete; el worker y sus WASM cargan desde `node_modules`. Con
`optimizeDeps.exclude: ['@vitela/core', '@vitela/viewer']` también funciona (prueba
desechable en Chromium dev, 2/2), pero no cambia nada. En el build tampoco hace falta
`worker.format: 'es'` ni `assetsInlineLimit: 0`: el worker sale en un solo fichero con el
formato por defecto (IIFE) y los dos `.wasm` y el pegamento de pdfium salen como assets. El
`vite.config` del ejemplo solo tiene `@vitejs/plugin-react` y las dos entradas HTML. No se
probaron otras versiones de Vite ni otros bundlers; el README remite a
`engineOptions.workerUrl`/`assetBaseUrl` si uno no sigue el `new URL(…)`.

Ejemplo consumidor: `examples/embed-vite` (no es miembro del workspace npm). Su
`package-lock.json` fija React, Vite y Playwright; `npm run setup`
(`scripts/install-packed.mjs`) hace `npm pack` de `@vitela/core` y `@vitela/viewer` en
`.packs/` (su `prepack` limpia y construye `dist/`) y los instala con `npm install --no-save`
**encima**, sin enlaces de workspace y sin referencias `file:` en `package.json` ni en el
lockfile (su integridad cambiaría en cada build); `npm ls` muestra el `@vitela/core` del
viewer como `deduped` sobre el tarball. Copia además `compress/vector_only.pdf` a
`public/sample.pdf`. Dos páginas: `index.html` (`<VitelaViewer>` en `StrictMode`, abre
`/sample.pdf`, guarda en `onSave` y puede reabrir los bytes guardados como `source`) y
`vanilla.html` (`mountViewer` sin selector, con el handle en `window` para el test).
`npm run typecheck` comprueba las páginas contra los `.d.ts` del tarball
(`skipLibCheck: false`).

Flujos (`examples/embed-vite/e2e/embed.spec.ts`, 2 tests × 3 navegadores × 2 servidores =
12; proyectos `chromium-build` … `webkit-dev`, build + `vite preview` en el 4180 y
`vite` dev en el 4181):

- `<VitelaViewer>`: abre `sample.pdf` desde la URL (3 páginas), borra la primera en
  Organize, Save → "Saved sample.pdf.", la anfitriona recibe `sample.pdf: N bytes, starts
  %PDF-` y la marca de cambios desaparece; reabre los bytes y son 2 páginas. Sin errores de
  página.
- `mountViewer`: lo mismo con `viewer.save()` y `viewer.open(bytes, 'saved.pdf')` desde el
  test, sin "Open PDF"; `viewer.destroy()` vacía el contenedor y el worker se cierra
  (`page.workers()` pasa de 1 a 0, evento `close`).

CI (`web.yml`): tras el Playwright de la app (empaquetar limpia y reconstruye los `dist/`),
`npm ci` + `npm run setup`, `npm run typecheck` y `npx playwright test` en
`examples/embed-vite`, con los navegadores ya instalados; `examples/embed-vite/**` entra en
los `paths`, su lockfile en la caché de `setup-node` y sus informes en el artefacto de
fallo. Ningún paso existente cambia.

Gates (Windows 11, local):

- `@vitela/core`: `npm run typecheck`, `npm test` y `npm run build` en verde. Vitest
  **13 ficheros / 134 tests** (antes 13 / 132): los 2 de `terminate()` en `client.test.ts`,
  vistos en rojo antes de implementar.
- `@vitela/viewer`: `npm run typecheck`, `npm test` y `npm run build` en verde. Vitest
  **57 ficheros / 532 tests** (antes 54 / 505): `VitelaViewer.test.tsx` (15),
  `mountViewer.test.tsx` (2), `callbackDelivery.test.ts` (3), `DocumentWorkspace` (2),
  `useDocument` (3), `useSave` (1) y `SaveControls` (1). En rojo antes de implementar: los
  tres ficheros nuevos (no existía el módulo) y 5 tests de los existentes; dos de
  `useDocument` (abrir una fuente que no es `File` y el fallo al leerla) ya pasaban, porque
  el hook solo usaba `name` y `arrayBuffer()`: fijan ese contrato.
- `apps/web`: `npm run typecheck`, `npm test` (**6 / 19**, sin cambios) y `npm run build`
  en verde. No cambia ningún fichero de la app.
- Playwright de `apps/web` (Chromium, Firefox, WebKit): **148 pasan, 20 omitidos**, como en
  T-246. Una primera pasada dio 147 / 1 fallo / 20: el test conocido de
  `embed.spec.ts` "takes a file dropped on the editor…" en Firefox (`hint: false`), el
  mismo que arregla el PR #632, fuera de esta rama; con `--repeat-each=5` volvió a fallar 1
  de 15. No se tocó aquí.
- Playwright de `examples/embed-vite`: **12 pasan** (build y dev, tres navegadores).
- Los flujos del ejemplo se escribieron después de la API: no se vieron en rojo (sin la API
  no compilan).

Revisión (cuatro lentes, sin bloqueantes). Dos avisos se corrigieron en el mismo PR:

- **Apertura en vuelo al desmontar.** La limpieza de `useDocument` solo cerraba el documento
  en pantalla; uno que el motor terminaba de abrir después se guardaba y no se cerraba nunca
  (con un motor de la anfitriona, vivo hasta que ella lo pare). Ahora `current()` también
  exige que el hook siga montado y la apertura tardía se devuelve al motor. Es una bandera y
  no un `latest++`: StrictMode ejecuta esa limpieza una vez al montar, y con `latest++` la
  primera apertura de `<VitelaViewer>` se habría descartado.
- **Cambiar de motor con el editor montado.** Los handles se numeran por motor, y el
  workspace seguía usando el del motor anterior contra el nuevo (pintar, guardar o cerrar el
  documento de otro). `DocumentWorkspace` lleva ahora `key` por motor: el viejo cierra su
  documento en su motor y el nuevo empieza vacío; la última petición (`source` u `open`) se
  vuelve a abrir en cuanto el nuevo motor está listo. `useEngineStatus` ata el estado al
  cliente que lo informó: un cliente nuevo está "cargando" hasta que diga lo suyo (antes
  heredaba el "listo" del anterior).
- Tests nuevos, vistos en rojo: apertura terminada tras desmontar (`useDocument`), cambio de
  motor (`VitelaViewer`) y cliente nuevo vuelve a "cargando" (`useEngineStatus.test.ts`,
  fichero nuevo con 3). Dos más fijan StrictMode (ya pasaban). Vitest del viewer:
  **58 ficheros / 539 tests**.
- Quedan como seguimiento: cambiar de documento o destruir sin avisar de cambios sin
  guardar (no hay `dirty` en el handle), el ejemplo de `onSave` del README que guarda por un
  nombre que el usuario controla, `callbackDelivery` reutilizando el destino `'copy'` y
  descargas por URL sin cancelar.

Lo que condiciona T-248:

- El ejemplo es el sitio para la verificación de T-248: cuando los paquetes estén en npm,
  `npm run setup` se cambia por `npm install @vitela/core @vitela/viewer` de las versiones
  publicadas, con los mismos 12 flujos.
- Publicar los dos a la vez y con la misma versión: `@vitela/viewer` depende de
  `@vitela/core` **exacto** (`0.1.0`), y `terminate()` solo existe desde este cambio. Un
  viewer publicado contra un core anterior fallaría al desmontar.
- `npm pack` del viewer incluye ahora `README.md` (npm lo añade solo); `files` no cambia.
- `onSave` no distingue "Save anyway" de "Save a copy" en la pregunta de firma: los dos
  entregan los bytes con el nombre del documento (README, "Not supported yet").
- React sigue siendo `peerDependency` exacta (`19.3.1`); un consumidor con otra 19.x choca
  con ella al instalar (no probado). Relajarla a `^19` es decisión aparte.

### T-248 — resultado

**En curso: el código está, pero todavía no se ha publicado nada** (2026-11-23,
`npm view @vitela/core` → E404). T-248 se marca ✓ cuando la primera versión esté en npm y el
job `verify` la haya instalado desde el registro en verde.

- **Tag `npm-v<semver>`**, con cadencia propia (como `web-deploy`): no casa con el `v*` de
  `release.yml`. Los dos paquetes salen con la versión del tag. Publicar una versión es subir
  `version` en los dos `package.json` y la dependencia exacta del viewer (más `apps/web` y el
  lockfile) en un PR, y después el tag sobre `main`.
- **`npm-release.yml`**: `version` (el tag está en `main`; `scripts/npm-release/check.mjs`
  exige que tag, core, viewer y la dependencia del viewer sean la misma versión, sin
  metadatos de build; prerelease → `next`, estable → `latest`) → `web` (todo `web.yml` sobre
  ese commit) → `publish` → `verify`. `workflow_dispatch` es un ensayo: comprueba, prueba y
  sube el artefacto, sin publicar, aunque se lance sobre un tag.
- **Se empaqueta una vez y se publica lo probado.** `web.yml` gana `workflow_call` con
  `upload-packages`: sube los dos tarballs de `examples/embed-vite/.packs`, los mismos que
  instaló y probó el ejemplo, y `publish` publica esos ficheros sin reconstruir. `.packs` es
  un directorio oculto, que `upload-artifact` ignora si no se pide
  `include-hidden-files`.
- **`npm publish --dry-run` en cada `web.yml`**, sobre esos tarballs. No necesita sesión (npm
  solo avisa), ni genera procedencia. Lleva `--force` porque, sin él, npm rechaza una versión
  ya publicada **también en el ensayo**: tras la primera publicación, todos los PR saldrían en
  rojo hasta subir la versión.
- **`scripts/npm-release/publish.mjs`** mira los dos paquetes en el registro antes de publicar
  ninguno: ausente (E404) → publica con `--provenance`; mismo sha512 → lo salta (repetir los
  jobs fallidos es seguro); otros bytes → falla; cualquier otro error de `npm view` → falla,
  nunca cuenta como "no publicado". Una estable por debajo del `latest` actual se rechaza.
  Tras un fallo a medias: **"Re-run failed jobs"**, no "Re-run all jobs" (volvería a
  empaquetar, y si los bytes cambian se niega).
- **Sin token de npm**: publicación de confianza (OIDC). npm comprueba el nombre del fichero
  del workflow que arranca la ejecución, así que `npm publish` vive en `npm-release.yml` y
  renombrarlo rompe la publicación.
- **Ejemplo consumidor:** `npm run setup` sigue empaquetando; `npm run setup:registry --
  <versión>` instala esa versión del registro (reintenta hasta cinco minutos si aún no se ha
  propagado) y comprueba que viene de ahí. `verify` lo usa con el mismo typecheck y los
  mismos 12 flujos.

Gates (Windows 11, local):

- `node --test scripts/npm-release/*.test.mjs examples/embed-vite/scripts/*.test.mjs`:
  **29 pasan**, vistos en rojo antes de implementar. En CI corren en `web.yml`.
- `actionlint` sobre los dos workflows: sin avisos.
- `npm run setup` + typecheck en el ejemplo y `npm publish --dry-run` de los dos tarballs: verde.
- `publish.mjs` con `npm view` real y `npm publish` simulado: contra npmjs.org (E404 → los dos
  por publicar); contra un registro local, los dos ya publicados (salta), solo core (publica el
  viewer) y otros bytes (falla); registro inaccesible (falla).
- `setup:registry` contra ese registro local, con un 404 antes de aparecer: un reintento y
  typecheck y Playwright del ejemplo, **12 pasan**.
- **No comprobado:** los workflows no han corrido en GitHub; OIDC, la procedencia y la
  publicación real solo se pueden ver en la primera.

#### Pasos del usuario

1. **Crear la organización `vitela`** en npmjs.com (plan gratuito, paquetes públicos), con 2FA
   en la cuenta.
2. **Crear cada paquete a mano, una vez.** El publicador de confianza solo se puede configurar
   en un paquete que ya existe, así que se publica un `0.0.0` sin contenido desde un directorio
   vacío (Git Bash; `npm login` antes):

   ```bash
   npm login
   for name in core viewer; do
     dir="$(mktemp -d)"
     printf '{"name":"@vitela/%s","version":"0.0.0","description":"Placeholder: the first real version is 0.1.0.","license":"MIT OR Apache-2.0"}\n' "$name" > "$dir/package.json"
     (cd "$dir" && npm publish --access public)
   done
   npm view @vitela/core version    # 0.0.0
   npm view @vitela/viewer version  # 0.0.0
   ```

3. **GitHub:** crear el entorno `npm` (Settings → Environments), con despliegue limitado a
   tags `npm-v*` y **un revisor obligatorio**: un tag ejecuta el `npm-release.yml` de su propio
   commit, así que las comprobaciones del workflow no frenan a quien empuje un tag sobre un
   commit que las quite; la aprobación del entorno sí. En el ruleset `release-tags`, añadir
   `refs/tags/npm-v*` junto a `refs/tags/v*`, para que esos tags tampoco se puedan mover ni
   borrar. Conviene lanzar aquí el ensayo: Actions → npm-release → Run workflow.
4. **Publicación de confianza**, solo cuando se vaya a hacer el paso 5 enseguida: una
   configuración nueva caduca si no hay una primera publicación con éxito en **2 días**. Pide
   2FA. Desde el 3 de septiembre de 2026 las configuraciones nuevas solo permiten la
   publicación por etapas (`npm stage publish`), así que hay que permitir la directa. No se
   pueden editar: si algo está mal o caduca, se borra y se crea otra.

   ```bash
   npx npm@^11.15 trust github @vitela/core --file npm-release.yml --repo PabloBaroParra/vitela --env npm --allow-publish
   npx npm@^11.15 trust github @vitela/viewer --file npm-release.yml --repo PabloBaroParra/vitela --env npm --allow-publish
   ```

5. **Publicar** (el tag, sobre `main` ya fusionado; después, aprobar el entorno `npm`):

   ```bash
   git fetch origin main
   git tag -a npm-v0.1.0 -m "npm 0.1.0" origin/main
   git push origin refs/tags/npm-v0.1.0
   ```

### Fase 4 — tareas

| Tarea | Entrega | Hecho cuando |
|---|---|---|
| **T-249** ✓ (#656) | Core: mover `protect_to_bytes` (`pdf-ffi/src/document.rs`) y la orquestación de extraer páginas (`pdf-ffi/src/extract.rs`) a `pdf-save`; `pdf-ffi` delega. | Nativo: cero cambio de comportamiento (tests de fachada sin tocar); `cargo check --target wasm32-unknown-unknown -p pdf-wasm`. |
| **T-250** ✓ | Comprimir: presets del core, rechazo del cifrado, aviso de firmas, copia más pequeña como descarga. | Playwright: un escaneo sale más pequeño, se descarga y abre; cifrado → rechazado en el diálogo sin descarga; firmado → "Compress anyway". |
| **T-251** ✓ | Proteger (dos contraseñas, aviso de firmas). | Playwright: proteger, reabrir con cada contraseña y ver sus permisos. |
| **T-252** ✓ | Exportar imágenes de página (`renderPage` + `encode_rgba_image`, selección de páginas, nombres, límite de tamaño). | Playwright: PNG y JPEG de un rango, con los nombres y el tamaño esperados. |
| **T-253** ✓ | Insertar página en blanco (`PageEdit::InsertBlank` + botón en Organize). | Playwright: insertar, deshacer, guardar y contar páginas. |
| **T-254** ✓ | Extraer y dividir (varias descargas). | Playwright: cada parte se descarga y abre con sus páginas. |
| **T-255** ✓ | Metadatos. | Playwright: editar, guardar y releer. |
| **T-256** ✓ | Marca de agua (texto e imagen). | Playwright: aplicar las dos, guardar y verla al reabrir. |
| **T-257** ✓ | Imprimir (render + `window.print`). | Playwright: la vista de impresión tiene una imagen por página. |
| **T-258a** ✓ (#666) | Core: la orquestación de la edición de contenido sale de `pdf-ffi` a `pdf-save` (`content_edit.rs`) y `pdf-edit` (`group_text_paragraphs`); `pdf-ffi` delega. | Nativo: cero cambio de comportamiento. |
| **T-258b** ✓ | Modelo de contenido en wasm (`CheckpointSession`, `readPageContent`, bloques y párrafos, `textRunStyle`, familias de fuente, `contentBaseGeneration`) y modo **Edit content** de solo lectura en el visor. | `wasm-bindgen-test` + Vitest contra pdfium real; Playwright del modo. |
| **T-259** ✓ | Texto: reescribir, mover, borrar, estilo. | Playwright: cada edición se guarda y se relee. |
| **T-260** ✓ | Imágenes: mover, redimensionar, borrar, reemplazar, insertar. | Playwright: cada edición se guarda y se relee. |
| **T-261** ✓ | Insertar texto, y bloques/párrafos. | Playwright: insertar y reescribir un párrafo. |
| **T-262** ✓ | Selección y portapapeles de contenido. | Playwright: copiar, cortar y pegar entre páginas. |
| **T-263** ✓ | Trazados: mover, redimensionar, borrar. | Playwright: cada edición se guarda y se relee. |

T-258 bloquea T-259…T-263. T-249 bloquea T-251 y T-254. Las demás son independientes.

**Fase 4: todas las filas hechas** (T-249…T-263). Queda cerrada cuando T-262 (#675) y T-263
estén fusionadas y sus flujos Playwright en verde en CI.

### T-250 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Compress") y en
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API). Ninguna regla
nueva en el core: `pdf_save` ya tenía las tres llamadas. Lo que condiciona lo que sigue:

- **`pdf-wasm`** (`model/compress.rs`, `engine/compress.rs`, `dto/compress.rs`): depende de
  `pdf-compress` (solo por `CompressPreset` y el informe, como `pdf-ffi`). **Mismo orden que
  `pdf-ffi`:** `compression_blocker`, `compressed_save_will_invalidate_signatures` (no la
  pregunta ordinaria: un firmado sin editar contesta `true`) y `save_document_compressed`,
  sobre el mismo `SaveInput` que `saveToBytes` (que pasa a `with_save_input`, compartido),
  con las ediciones pendientes y `SaveIntent::Default`. Superficie: `compressPresets()` (función
  libre, nombres `lossless`/`balanced`/`small` en el orden del core), `compressionRefusal`,
  `compressionInvalidatesSignatures` y `saveCompressedToBytes(doc, preset, invalidateSignatures)`
  → `CompressedSave` (`beforeBytes`, `afterBytes`, `savedBytes`, `reduced`, `refusals`,
  `takeBytes()`). Un preset desconocido → `UnsupportedOperation`.
- **Decisiones.** El preset cruza como texto, no como enum de wasm-bindgen (números en JS).
  Los tamaños son `f64`, no `BigInt`: un módulo wasm32 no pasa de 4 GiB. El rechazo es
  `{ kind, message }`: `encryptedDocumentNotRewritable`, `signaturesWouldBeInvalidated` u
  `other` (la lista del core es abierta; su frase llega igual). `graft_warnings` se descarta,
  como en `pdf-ffi`. **Sin camino para el cifrado**: la web guarda siempre con
  `SaveIntent::Default`, así que un PDF protegido se rechaza en el diálogo (como Linux); no hay
  "quitar la protección y comprimir".
- **`@vitela/core`:** `Engine` gana `compressPresets`, `compressionRefusal`,
  `compressionInvalidatesSignatures` y `saveCompressedToBytes` (`CompressedDocument`, los bytes
  transferidos, no copiados); `worker/compress.ts` decodifica y libera los objetos wasm (un
  preset que no conoce se omite, un rechazo desconocido pasa a `other`). Tipos nuevos
  exportados: `CompressPreset`, `CompressRefusal`, `CompressRefusalKind`,
  `CompressedDocument`. Comprimir no es un paso: ni refresco ni historial.
- **`@vitela/viewer` (`features/compress/`):** puro con tests `compressText.ts` (tamaños en
  potencias de diez, porcentaje truncado, `<nombre>-compressed.pdf` como Android, frases);
  contenedor `useCompress`; vistas `CompressDialog` (`ModalShell`, un radio por preset,
  Balanced elegido) y `CompressControls`. **El orden de los nativos:** las dos preguntas al
  abrir (la de firmas solo si no hay rechazo; si falla, rechaza en vez de suponer "sin
  firmar"), la ejecución, y la copia **solo si salió más pequeña**; si no, "Nothing to gain…" y
  no se escribe nada. Con firmas el botón dice "Compress anyway" y confirmar es el
  consentimiento. Textos de Android (que son los de Windows y GTK). El botón va en la barra
  del documento, junto a Save, en las dos pantallas; espera a la cola de ediciones como Save
  (`useSave` gana `settleEdits`). La entrega usa el `Delivery` de siempre con un destino nuevo
  `newFile` (siempre descarga con ese nombre exacto; con `onSave` del anfitrión, llega ahí con
  ese nombre y no cuenta como guardado).
- **Medido, no supuesto:** el escaneo de 200 dpi a `balanced` baja de 237 961 a 153 876 bytes
  (−35 %), no a la mitad. Y **"nada que ganar" no se alcanza con fixtures reales a través de
  un guardado**: el guardado ordinario escribe suelto y el reempaquetado siempre gana algo
  (`already_packed.pdf`: 2 931 → 2 688; un PDF de una página: 690 → 627), así que ese caso
  queda cubierto solo con Vitest. De paso: con `already_packed.pdf` la copia "comprimida"
  (2 688) es **mayor que el archivo abierto** (2 601), porque el informe compara con el
  guardado, no con el disco; es la regla del core y de los nativos ("uncompressed", nunca
  "the original").
- **Teléfono (≤ 600 px):** con el botón nuevo los controles ya no cabían junto al nombre (a
  390 px el nombre quedaba en 0 px y fallaban 12 E2E de `comments` en los tres navegadores).
  Ahora Compress muestra solo su icono (la palabra sigue siendo su nombre accesible) y los
  controles de la barra se aprietan (relleno lateral 8 px, hueco 6 px), así que la barra
  **sigue en una fila** y el nombre vuelve a tener el sitio de antes. **No vale una segunda
  fila para las líneas de estado** (se probó y entró en el PR #655): cada píxel de alto de la
  barra sale de las páginas y del panel de Comments que va debajo (`max-height: 45 %`); a
  390 × 600 la barra pasó de 42 a 64 px, el panel de 87 a 77 px y su primera fila quedó bajo
  la cabecera fija (53 px), que se tragaba el clic: `comments.spec.ts:192` falló en CI en
  los tres navegadores (en Windows pasaba por las fuentes). Con una fila la barra vuelve a
  42 px y el panel a 87, lo mismo que antes de T-250. Ese margen (33 px bajo la cabecera para
  una fila de 35) ya era justo antes de T-250 y no se toca aquí.
- **Tests:** `pdf-wasm` 7 nativos nuevos (`model/compress.rs`) y 6 `wasm-bindgen-test`
  (`tests/compress.rs`); `@vitela/core` 16 Vitest nuevos, uno contra pdfium real (escaneo
  comprimido con un giro pendiente, reabierto girado, el giro aún se deshace, preset
  desconocido rechazado); `@vitela/viewer` 21 nuevos. Vistos en rojo antes de implementar
  (los de Rust, por no compilar; los de `compressText`, `CompressControls`, `delivery`,
  `worker/compress`, `documentEngine`, `handler` y `client`, fallando).
- **E2E (`e2e/compress.spec.ts`, 4 × 3):** el escaneo a Small baja de tamaño, se descarga como
  `scan_200dpi-compressed.pdf`, empieza por `%PDF-` y reabre con una página pintada; borrar
  una página en Organize y comprimir (Lossless) da una copia de 2 páginas y la edición sigue
  sin guardar y deshacible; `aes_128_user_and_owner.pdf` → alerta del core, Compress y los
  radios desactivados, Escape devuelve el foco al botón, ninguna descarga; firmado → aviso,
  Cancel, y "Compress anyway" descarga una copia más pequeña que abre.
- **Verificado en local (Windows):** `cargo fmt --all -- --check`; `cargo clippy --locked -p
  pdf-wasm --all-targets -- -D warnings` nativo y `--target wasm32-unknown-unknown` (con
  `PDFIUM_WASM_DIR`); `cargo check --locked --target wasm32-unknown-unknown -p pdf-wasm`;
  `cargo test -p pdf-wasm` 98 pasan. `@vitela/core`: `build:wasm`, `typecheck`, Vitest 15 /
  166. `@vitela/viewer`: `tsc -b`, Vitest 74 / 748. `apps/web`: `typecheck`, Vitest 6 / 19,
  `build`; Playwright completo (Chromium, Firefox, WebKit) **211 pasan, 20 omitidos** (los
  de File System Access fuera de Chromium), `zero-network` incluido; `compress` + `comments`
  con `--repeat-each=3`: 108 / 108.
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; compilan con clippy
  wasm32 y los corre el job `wasm32` de `core.yml`); `linux-gtk`, Android,
  `examples/embed-vite`.
- **No cubierto / pendiente:** la herramienta `/compress` de la portada sigue en `web: false`
  (como `annotate` y `forms`, que tampoco se han encendido: decisión aparte); sin lector de
  pantalla real.

### T-252 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Export images"),
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API) y
[`packages/vitela-viewer/README.md`](../packages/vitela-viewer/README.md) (qué llega a
`onSave`). Ninguna regla nueva en el core: `pdf_save` ya tenía la gramática de páginas, los
nombres y `encode_rgba_image`, y `pdf_render` el techo de raster. Lo que condiciona lo que sigue:

- **`pdf-wasm`** (`model/export.rs`, `engine/export.rs`): la superficie de `pdf-ffi`
  (`export.rs` + `output.rs`) con una diferencia forzada por el target:
  `pdf_save::export_page_as_image` es solo nativo (usa el renderer con hilos), así que la
  página la pinta el `SyncRenderer` del engine y la codifica `encode_rgba_image` en Rust, no
  un canvas. Funciones libres `imageExportFormats()` (`png`, `jpeg`), `parsePageSelection`
  (rechazo nuevo `InvalidPageSelection`, con la frase del core) y `pageImageFileName`; en el
  `Engine`, `firstPageTooLargeToExport`, `openImageExport` → id, `exportPageImage(export,
  page, dpi, format)` → bytes y `closeImageExport`. El formato cruza como texto, como el
  preset de T-250.
- **Qué se dibuja: la instantánea de salida, como Android y Windows.** Lo que escribiría un
  guardado ahora (`with_save_input` con la firma reconocida, como `pdf-ffi`
  `output_snapshot`: no se escribe nada, solo se rasteriza), abierta una vez por exportación
  en un mapa propio del engine, aparte de los documentos. Así entran las ediciones de página,
  las anotaciones y los rellenos sin guardar (el preview en pantalla deja fuera las
  anotaciones de la sesión). No es un paso: ni historial ni refresco. El permiso es el de
  extracción de texto, con la frase de `pdf-ffi`.
- **Sin calidad JPEG:** el codificador del core no la tiene, ni los otros shells. DPI 72–400,
  150 por defecto, caja vacía = 150 (el mismo techo y la misma razón que los nativos: Letter
  a 600 DPI pasa de 32 Mpx).
- **`@vitela/core`:** `Engine` gana `imageExportFormats`, `parsePageSelection`,
  `pageImageFileNames` (los nombres de varias páginas en una sola petición),
  `firstPageTooLargeToExport`, `openImageExport`, `exportPageImage` (bytes transferidos, no
  copiados) y `closeImageExport`. Tipos exportados `ImageExportFormat`, `ImageExportHandle` y
  la constante `INVALID_PAGE_SELECTION`.
- **`@vitela/viewer` (`features/exportImages/`):** puro con tests `exportText.ts` (borrador,
  lectura del DPI, frases de Android) y `planExport.ts` (el plan en el orden de los nativos:
  DPI, permiso, páginas, techo, nombres; un rechazo deja el diálogo abierto con lo tecleado);
  contenedor `useExportImages`; vistas `ExportImagesDialog` (`ModalShell`: Pages, Format,
  Resolution) y `ExportImagesControls`. El botón va en la barra, junto a Compress, en las dos
  pantallas. Espera a la cola de ediciones como Save. "Current
  page" es la última página que mostró el visor (`ReportCurrentPageContext`, nuevo), recortada
  a la última. Se para en la primera página que falla y dice cuántas se escribieron; la
  instantánea se cierra siempre. La caja de DPI es de texto (solo dígitos, tres como máximo,
  como Android): con `type="number"` la validación del navegador frenaba el envío con su
  globo antes de que el diálogo dijera por qué.
- **Entrega: una descarga por página, no un zip.** No hay escritor zip en el árbol de
  dependencias (ni npm ni Cargo) y meterlo sería solo para esto; los nativos escriben un
  archivo por página en una carpeta, y una descarga por página es lo más parecido. Cada
  imagen sale por el `Delivery` de siempre como `newFile`, que gana un `type` (tipo MIME;
  PDF si falta) para etiquetar el blob. **Anfitrión:** `onSave` recibe ahora un tercer
  argumento `type` (`application/pdf`, `image/png`, `image/jpeg`) y una llamada por página,
  en orden, esperando a cada una; un rechazo para la exportación. Es aditivo: un `onSave` de
  dos parámetros sigue funcionando. El README del viewer tiene la tabla de qué llega.
- **Teléfono (≤ 600 px): menú "More tools".** Sobre el arreglo de T-250 (664dd20: la barra en
  **una** fila, porque cada píxel de alto sale de las páginas y del panel de comentarios),
  un segundo icono no cabía junto a Save. Un primer intento movía los iconos a otra fila con
  `order`, que además dejaba el orden del tabulador distinto del visual (WCAG 2.4.3);
  descartado. Ahora, en teléfono, Compress y Export images se ocultan (`display: none`: fuera
  del tabulador y del árbol de accesibilidad) y en su lugar del DOM aparece un botón de menú
  **More tools** (`ui/ToolsMenu.tsx`, patrón "menu button" de la APG: `aria-haspopup`,
  `aria-expanded`, `aria-controls`; Enter, Espacio o ↓ abren en el primero, ↑ en el último;
  ↑/↓ con vuelta, Inicio/Fin, Escape cierra y devuelve el foco, Tab cierra; un toque fuera
  cierra; una herramienta ocupada sale con `aria-disabled`). Al elegir, el foco vuelve al
  botón **antes** de la acción, así que el diálogo que abre lo devuelve allí también. Es el
  sitio de las herramientas que vienen (Protect, Extract…): se añaden como una entrada más. En
  escritorio no cambia nada. Los iconos de Compress de T-250 ya no se muestran en ningún
  ancho y se quitaron.
- **Tests:** `pdf-wasm` 8 nativos nuevos (`model/export.rs`) y 4 `wasm-bindgen-test`
  (`tests/export.rs`); `@vitela/core` 11 Vitest nuevos, uno contra pdfium real (un subrayado
  sin guardar cambia los bytes del PNG, un giro sin guardar da 1584 × 1224 a 144 DPI, JPEG
  con su firma, rango rechazado por nombre, el historial intacto); `@vitela/viewer` 36
  nuevos (8 de `ToolsMenu`, 1 del menú en `DocumentWorkspace`). Vistos en rojo antes de implementar: los de Rust (por no compilar), `documentEngine`
  (6 fallando), `download`/`delivery`/`callbackDelivery` (3 fallando), los de
  `features/exportImages/` y `ToolsMenu` (sin módulo), el menú del workspace (fallando) y la
  caja de DPI (3 fallando con `type="number"`).
  Los de `handler`, `client` y el de pdfium real se escribieron antes que su código pero no
  se vieron en rojo por separado.
- **E2E (`e2e/export-images.spec.ts`, 4 × 3):** la página actual (tras bajar a la 3) en PNG
  a 144 DPI es una descarga `vector_only-3.png` de 1224 × 1584; el rango `2-3` en JPEG a
  72 DPI son `vector_only-2.jpg` y `vector_only-3.jpg` de 612 × 792 (leídos de la cabecera
  de cada archivo); una página girada en Organize sin guardar sale apaisada y la edición
  sigue sin guardar y deshacible; `2-9` muestra la frase del core y 600 DPI el rechazo del
  rango, sin ninguna descarga, y Escape devuelve el foco al botón. **`e2e/phone-bar.spec.ts`
  (2 × 3), a 390 × 600:** la barra es una sola fila (todos sus hijos visibles comparten franja
  y ninguno se sale), su orden en pantalla es el del DOM, Tab va Organize → More tools → Save,
  Compress y Export images están ocultos y Save visible; el menú se maneja con teclado (Enter
  abre en Compress, ↓, Enter abre Export images, Escape devuelve el foco a More tools; Escape
  cierra el menú) y con un toque abre Compress.
- **Verificado en local (Windows):** `cargo fmt --all -- --check`; `cargo clippy --locked -p
  pdf-wasm --all-targets -- -D warnings` nativo y `--target wasm32-unknown-unknown` (con
  `PDFIUM_WASM_DIR`); `cargo check --locked --target wasm32-unknown-unknown -p pdf-wasm`;
  `cargo test -p pdf-wasm` 106 pasan. `@vitela/core`: `build:wasm`, `typecheck`, Vitest 15 /
  177. `@vitela/viewer`: `build`, `tsc -b`, Vitest 78 / 784. `apps/web`: `typecheck`, Vitest
  6 / 19, `build`; Playwright completo (Chromium, Firefox, WebKit) sobre 664dd20 **229 pasan,
  20 omitidos** (los de File System Access fuera de Chromium), `zero-network` y `comments`
  390 × 600 incluidos; `comments`, `phone-bar`, `compress` y `export-images` con
  `--repeat-each=3`: 162 / 162. (Antes del menú, una ejecución completa tuvo un fallo de
  `draw` en Chromium, una alerta de texto libre que no se repitió después.)
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; compilan con clippy
  wasm32 y los corre el job `wasm32` de `core.yml`); `linux-gtk`, Android,
  `examples/embed-vite`; el aviso del navegador de "varias descargas" (Playwright las acepta
  todas).
- **No cubierto / pendiente:** el rechazo por página demasiado grande solo se cubre con
  Vitest y con el test nativo (a 400 DPI ningún fixture pasa del techo); la herramienta de
  la portada (`/pdf-to-image`) sigue en `web: false`, como `/compress`.
- **Revisión (después de #657):**
  - **Recuento de páginas:** `openImageExport` devuelve `{ handle, pageCount }` (DTO
    `OpenedImageExport` en `pdf-wasm`) y el plan cuenta las páginas de la instantánea,
    abierta tras asentarse la cola, no las del documento tal como lo vio el viewer (podía
    no haber alcanzado una edición todavía). Lo que no necesita instantánea (DPI, permiso)
    se comprueba antes (`precheckExport`); un rechazo del plan cierra la instantánea.
  - **Cancelar y desmontar:** **Cancel export** mientras corre (la página en curso sale;
    "Export cancelled. N written."); al desmontarse el editor el bucle para y la
    instantánea se cierra.
  - **Foco en teléfono:** al cerrar el diálogo, si el botón propio está oculto
    (`checkVisibility`), el foco va a **More tools** (`ui/focusShown.ts`), en Compress y en
    Export images. Playwright 390 × 600: tras Cancel de Compress y tras una exportación
    lanzada desde el menú, el foco está en More tools.
  - **Formatos del core:** el diálogo pinta los de `imageExportFormats` (PNG primero si
    está; si la pregunta falla, rechaza en vez de suponer).
  - **Descargas bloqueadas:** solo documentado (README de `apps/web`): la página no puede
    saber si el navegador permitió cada descarga.
  - `sentence` pasa a `ui/text.ts` (compartido); fuera las líneas en blanco dobles.
  - **Puertas:** `cargo fmt --check`, clippy `pdf-wasm` nativo y wasm32, `cargo test -p
    pdf-wasm` 106; `@vitela/core` `build:wasm`, `typecheck`, Vitest 177; `@vitela/viewer`
    `tsc -b`, Vitest 792; `apps/web` `typecheck`, Vitest 19, `build`; Playwright completo en
    los tres navegadores **232 pasan, 20 omitidos**. Vistos en rojo antes del código: los
    12 de `ExportImagesControls`, el de `documentEngine`, el de foco de Compress y los
    `wasm-bindgen-test` (por no compilar); los dos Playwright de foco nuevos no se vieron
    en rojo por separado.

### T-251 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Protect"),
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API) y
[`packages/vitela-viewer/README.md`](../packages/vitela-viewer/README.md) (qué llega a
`onSave`). Ninguna regla nueva en el core: T-249 ya dejó en `pdf_save::protect` las dos
contraseñas, AES-128 con `GRANTED_PROTECTION_PERMISSIONS` y la pregunta de firmas. Lo que
condiciona lo que sigue:

- **`pdf-wasm`** (`model/protect.rs`, `engine/protect.rs`): `protectionRefusal(doc)` → frase o
  `undefined`, `protectionInvalidatesSignatures(doc)` y `protectToBytes(doc, openPassword,
  permissionsPassword, invalidateSignatures)` → bytes, sobre un `SaveSource` como el de
  `pdf-ffi` (`with_save_source`, nuevo en `model/edit.rs`; `with_save_input` pasa a usarlo), con
  las ediciones pendientes. **La puerta de permiso vive en Rust**, no en TS: la de Windows y
  Android (`content_editing_is_allowed`, el bit de modificar contenido; un propietario siempre
  pasa), con la frase de los nativos ("this document does not permit protection changes");
  `protectToBytes` la repite (`EditNotPermitted`). Tipo de error nuevo **`InvalidPasswords`**
  para `ProtectError::MissingPassword`/`SamePasswords` con la frase del core (en `pdf-ffi` son
  `InvalidSaveRequest`); `ProtectError::Save` conserva los tipos del guardado. Sin DTO: cruzan
  texto, booleano y bytes.
- **Documento ya cifrado:** como Windows y Android, decide el permiso de modificar, no el
  cifrado. `aes_128_user_and_owner.pdf` con su contraseña de usuario (su `/P` no permite
  modificar) se rechaza en el diálogo; con la de propietario, o un PDF cifrado que sí permite
  modificar, se protege con las contraseñas nuevas (tests nativos). GTK pregunta además
  `full_rewrite_refusal`; con un contexto nuevo completo el core no lo necesita, así que no se
  añade.
- **`@vitela/core`:** `Engine` gana `protectionRefusal`, `protectionInvalidatesSignatures` y
  `protectToBytes(handle, { openPassword, permissionsPassword }, invalidateSignatures)` (un
  objeto, para que las dos contraseñas no se crucen de sitio; los bytes transferidos). Tipo
  `ProtectionPasswords` y constante `INVALID_PASSWORDS` exportados. Las contraseñas viajan en
  la única petición `protect` y no se guardan en ningún lado; ni el cliente ni el worker
  registran nada (un test comprueba que la respuesta de error no las lleva).
- **`@vitela/viewer` (`features/protect/`):** puro con tests `protectText.ts`
  (`<nombre>-protected.pdf` como Android, línea de estado); contenedor `useProtect`; vistas
  `ProtectDialog` (`ModalShell`: los dos campos con `type="password"` y
  `autocomplete="new-password"`, textos de Android) y `ProtectControls`. **El orden de los
  nativos:** las dos preguntas al abrir (la de firmas solo si no hay rechazo; si falla, rechaza
  en vez de suponer "sin firmar"), las contraseñas y la copia. **Ninguna regla de contraseñas
  en TS:** el diálogo manda lo tecleado y, si el core lo rechaza (`InvalidPasswords`), muestra
  su frase con el diálogo abierto y lo tecleado intacto (Android y Windows comprueban antes del
  selector de archivo porque lo tienen; la web no tiene selector, así que la respuesta del core
  llega antes de escribir nada). Sin campos de confirmación: ningún shell nativo los tiene.
  **Contraseñas fuera del estado de React:** viven en los campos (no controlados) del diálogo,
  salen solo en la llamada, y los campos se vacían al desmontarse. Con firmas el botón dice
  "Protect anyway" y confirmar es el consentimiento. Espera a la cola de ediciones como Save.
  Botón en la barra tras Export images y entrada **Protect** en **More tools** (oculto en
  teléfono como los otros dos).
- **No se reabre en sitio (diferencia con los nativos):** Android, Windows y GTK reabren el
  archivo protegido como sesión con las dos contraseñas. En la web la copia es una descarga
  (`newFile`), no el archivo de la sesión, así que el documento abierto sigue sin proteger, con
  sus ediciones y su historial, y la línea de estado lo dice ("The open document is not
  protected."). Para trabajar sobre la copia se abre: la app pide su contraseña y cualquiera de
  las dos la abre (el `open` de la web solo admite una; la de permisos abre como propietario).
- **Tests:** `pdf-wasm` 7 nativos nuevos (`model/protect.rs`) y 4 `wasm-bindgen-test`
  (`tests/protect.rs`); `@vitela/core` 9 Vitest nuevos, uno contra pdfium real
  (`vector_only.pdf` con una página borrada sin guardar → copia cifrada que pide contraseña, abre
  con cada una con 2 páginas y todos los permisos, contraseñas iguales rechazadas con la frase
  del core, el borrado aún se deshace); `@vitela/viewer` 12 nuevos (4 de `protectText`, 7 de
  `ProtectControls`, 1 del workspace; y el del menú ahora espera tres entradas). Vistos en rojo
  antes de implementar: los de Rust (por no compilar), los 8 de `client`/`handler`/
  `documentEngine` y los de `features/protect/` (sin módulo) y del workspace (2 fallando). Los
  `wasm-bindgen-test` y el de pdfium real se escribieron antes que su código pero no se vieron
  en rojo por separado.
- **E2E (`e2e/protect.spec.ts`, 4 × 3):** borrar una página en Organize y proteger descarga
  `vector_only-protected.pdf` (empieza por `%PDF-`, lleva `/Encrypt`), el foco vuelve a Protect
  y el borrado sigue sin guardar y deshacible; la copia rechaza una contraseña equivocada, abre
  con la de apertura (2 páginas, pintada, Delete de Organize activo: todos los permisos) y con
  la de permisos; contraseña ausente y repetida muestran las frases del core sin ninguna
  descarga, y al reabrir el diálogo los campos están vacíos; firmado → aviso, Cancel, y
  "Protect anyway" descarga una copia que abre con su contraseña;
  `aes_128_user_and_owner.pdf` con la contraseña de usuario → alerta, Protect y los campos
  desactivados, foco en Cancel, Escape lo devuelve a Protect. **`e2e/phone-bar.spec.ts`
  (+1 × 3), a 390 × 600:** Protect oculto en la barra, el menú lista Compress, Export images y
  Protect, el diálogo cabe en la pantalla con el foco en el primer campo y Cancel devuelve el
  foco a More tools; la barra sigue en una fila.
- **Verificado en local (Windows):** `cargo fmt --all -- --check`; `cargo clippy --locked -p
  pdf-wasm --all-targets -- -D warnings` nativo y `--target wasm32-unknown-unknown` (con
  `PDFIUM_WASM_DIR`); `cargo check --locked --target wasm32-unknown-unknown -p pdf-wasm`;
  `cargo test --locked -p pdf-wasm` 113 pasan. `@vitela/core`: `build:wasm`, `typecheck`,
  Vitest 15 / 186. `@vitela/viewer`: `tsc -b`, Vitest 81 / 804. `apps/web`: `typecheck`,
  Vitest 6 / 19, `build`; Playwright `protect` + `phone-bar` en los tres navegadores 24 / 24;
  Playwright completo (Chromium, Firefox, WebKit, con `CI=1` para no reutilizar un servidor
  ajeno) **247 pasan, 20 omitidos** (los de File System Access fuera de Chromium),
  `zero-network` y `phone-bar` incluidos.
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; compilan con clippy
  wasm32 y los corre el job `wasm32` de `core.yml`); `linux-gtk`, Android,
  `examples/embed-vite`.
- **No cubierto / pendiente:** la herramienta `/protect` de la portada sigue en `web: false`,
  igual que `/compress` y `/pdf-to-image`; sin editor de restricciones (`/P` siempre
  abierto, como en todos los shells); sin lector de pantalla real.

### T-253 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Organize", **Add blank page**) y
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API). Ninguna regla
nueva en el core: `Command::insert_blank_page` ya mintaba el `PageId` desde el `Document` y
`pdf_save::check_edit` ya clasificaba la inserción. Lo que condiciona lo que sigue:

- **Semántica copiada de `pdf-ffi` (`FfiEditCommand::InsertBlankPage`):** una página A4
  (595 × 842), vertical u horizontal — lo único que ofrecen Windows (`PdfCoreEdit.InsertBlankPage`
  siempre con `FfiPageSize.A4`) y Android (`PageEdit.InsertBlank(index, landscape)`) —, insertada
  para quedar en la posición `index`; `index == pageCount` añade al final. El límite (`len`, no
  `len - 1`) se comprueba **antes** de mintar el id, como `pdf-ffi`, así que una posición mala no
  gasta ninguno; `PageIndexOutOfBounds` nombra la posición. Espacio de ids agotado →
  `UnsupportedOperation` (en `pdf-ffi` es `InvalidSaveRequest`, que la web no tiene). Un paso de
  deshacer; el redo conserva el mismo id.
- **Puerta:** la misma de Move/Remove, sin código nuevo: `PageEdit::InsertBlank` es
  `document_assembly + page_structure` (`EditClass`, verificado contra `EditClass::of` del
  comando construido en el test de paridad), así que `check_edit` pide el permiso de ensamblado
  (`EditNotPermitted`) y después `full_rewrite_blocker` (`UnsupportedOperation`: p. ej. un PDF
  cifrado abierto solo con la contraseña de usuario, aunque conceda ensamblado). `previewStale`
  queda en `true` y el worker refresca la vista dentro de la misma petición.
- **`pdf-wasm`** (`model/edit.rs`, `engine/edit.rs`): variante `PageEdit::InsertBlank { index,
  orientation }`; `PageEdit::command` pasa a `&mut Document` porque la inserción minta el id.
  JS: `EditCommand.insertBlankPage(index, landscape)`.
- **`@vitela/core`:** `PageEdit` gana `{ kind: 'insertBlank', at, orientation }` y se exporta el
  tipo `PageOrientation` (`'portrait' | 'landscape'`). Sin petición nueva: viaja por `applyEdit`.
- **`@vitela/viewer` (`features/organize/`):** **UX.** Los nativos ponen "+" en cada tarjeta
  ("insertar antes", con Vertical/Horizontal en Android y Windows) y un "añadir al final"
  (tarjeta final en Android, dos botones en la cabecera de Windows); GTK no tiene la acción en
  Organize. La web no tiene botones dentro de las tarjetas (son opciones de un `listbox` con la
  selección como sujeto de toda la barra), así que la acción es un botón-menú **Add blank page**
  en la barra (`ToolsMenu`, que gana `disabled`: `aria-disabled` y no se abre) con **Portrait
  A4** / **Landscape A4**, que inserta **después** de la selección (tras la última página de los
  bloques elegidos en Documents) o al final si no hay selección, y selecciona la tarjeta nueva.
  Así cubre los dos casos de los nativos (después de la página N = antes de la N+1; al final)
  en las dos vistas; en Documents la página sale como bloque "Blank pages". Sin el permiso el
  botón está desactivado como los demás y el aviso dice ahora "added, moved, rotated or deleted".
- **Tests:** `pdf-wasm` 4 nativos nuevos (inserción con `/MediaBox` A4 y sin contenido leídos
  con lopdf, horizontal añadida al final, un paso de deshacer/rehacer con el mismo id, rechazo
  por reescritura) y casos nuevos en paridad de `EditClass`, fuera de rango y sin permiso; 1
  `wasm-bindgen-test` nuevo (`tests/engine.rs`: id propio, render 595 × 842 sin tinta, 842 × 595
  la horizontal, guardar y reabrir) más casos en `page_edits_are_refused_by_kind`.
  `@vitela/core` 3 Vitest nuevos (2 de `documentEngine`, 1 contra pdfium real: id y bloque
  `blank` propios, render blanco, `/MediaBox` al reabrir, posición fuera de rango).
  `@vitela/viewer` 5 nuevos (4 de `OrganizeScreen`: después de una página, después de un rango
  en horizontal, deshacer, vista Documents; 1 de `ToolsMenu` desactivado) y el de documento
  bloqueado ampliado. Vistos en rojo antes de implementar: los de `documentEngine` (2), los de
  `OrganizeScreen` (5) y el de `ToolsMenu`. Los de Rust y el de pdfium real se escribieron
  junto a su código y no se vieron en rojo por separado.
- **E2E (`e2e/insert-blank.spec.ts`, 2 × 3):** con la página 1 seleccionada, Portrait A4 → 3
  tarjetas (`300`, `500` = A4, `400`), la nueva seleccionada ("Page 2 of 3 selected") y el foco
  de vuelta en el botón; Undo → 2, Redo → 3; el guardado tiene 3 páginas con exactamente un
  `/MediaBox [0 0 595 842]`, y al reabrirlo "3 pages", la página 2 sin un píxel de tinta y con
  proporción A4, la 1 con su texto. Documents → Landscape A4 → bloque "Blank pages, 1 page"
  seleccionado, guardado con un `/MediaBox [0 0 842 595]`. **`e2e/organize.spec.ts`:** el PDF
  sin permiso de ensamblado tiene Add blank page desactivado y forzar el clic no abre menú ni
  añade página. Helper nuevo `pageMediaBoxes` en `e2e/pdfObjects.ts`.
- **Verificado en local (Windows):** `cargo fmt --all -- --check`; `cargo clippy --locked -p
  pdf-wasm --all-targets -- -D warnings` nativo y `--target wasm32-unknown-unknown` (con
  `PDFIUM_WASM_DIR`); `cargo check --locked --target wasm32-unknown-unknown -p pdf-wasm`;
  `cargo test --locked -p pdf-wasm` 117 pasan. `@vitela/core`: `build:wasm`, `typecheck`,
  Vitest 15 / 189, `build`. `@vitela/viewer`: `tsc -b`, Vitest 81 / 809, `build`. `apps/web`:
  `tsc -b`, Vitest 6 / 19, `vite build`; Playwright completo (Chromium, Firefox, WebKit, con
  `CI=1`) **253 pasan, 20 omitidos** (los de File System Access fuera de Chromium),
  `zero-network` y `phone-bar` incluidos.
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; compilan con clippy
  wasm32 y los corre el job `wasm32` de `core.yml`); `linux-gtk`, Android,
  `examples/embed-vite`.
- **No cubierto / pendiente:** sin tamaño Carta ni personalizado (ningún shell nativo lo ofrece
  en Organize, aunque `FfiPageSize` lo admite); sin "insertar antes" por tarjeta (se cubre con
  seleccionar la página anterior); sin lector de pantalla real.

### T-254 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Organize", **Extract** y **Split**),
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API) y
[`packages/vitela-viewer/README.md`](../packages/vitela-viewer/README.md) (qué llega a
`onSave`). Lo que condiciona lo que sigue:

- **Core (`pdf-save`):** una función nueva, `pdf_save::extraction_refusal(&Document)` → las dos
  puertas de la extracción que no dependen de la selección (`ExtractionDenied`, luego
  `RewriteBlocked`), en el orden de `extracted_pages`, que ahora la usa. Así la web pregunta la
  puerta antes de abrir el diálogo con la frase del core (`rewrite_refusal` es privada) sin
  copiarla. `pdf-ffi` no cambia.
- **`pdf-wasm`** (`model/extract.rs`, `engine/extract.rs`, `dto/extract.rs`):
  `extractionRefusal(doc)` → frase o `undefined`, `extractSourceIsSigned(doc)` (de `base`, como
  `pdf-ffi`), `extractPagesToBytes(doc, pages)` sobre el `SaveSource` de la sesión (ediciones
  pendientes incluidas; poda un **clon**, sin paso de deshacer) y la función libre
  `planSplit(cuts, totalPages, documentName)` → `SplitPart { first, last, fileName }` (el
  `plan_split` de `pdf-ffi`: `resolve_split_cuts` + `split_parts` + `split_part_file_name`).
  Errores con el mapeo de `pdf-ffi`: selección vacía o desordenada → `InvalidPageSelection`,
  posición fuera → `PageIndexOutOfBounds`, permiso o cifrado → `UnsupportedOperation` con la
  frase del core, guardado → los del guardado; un corte que el core rechaza →
  `InvalidPageSelection` con su frase.
- **Cifrado y firmas, como los nativos:** extraer de un PDF protegido da un PDF **protegido**
  (`SaveIntent::Default` del core: el bit 5 permite sacar contenido, no entregarlo sin
  protección); abierto solo con la contraseña de usuario se rechaza (la reescritura no puede
  recifrarlo); con las dos, la copia pide la misma contraseña. De un PDF firmado se extrae igual
  y el estado lo dice ("…signature no longer verifies"), sin pregunta, como Android y Windows.
- **`@vitela/core`:** `Engine` gana `extractionRefusal`, `extractSourceIsSigned`,
  `extractPages(handle, pages)` (bytes transferidos) y `planSplit(cuts, totalPages, name)`;
  tipo `SplitPart` exportado; `worker/extract.ts` convierte y libera los objetos wasm.
- **`@vitela/viewer` (`features/extract/`):** puro con tests `extractText.ts` (nombre
  `<nombre>-extract.pdf` de Android y Windows, pistas y estados con las palabras de Android);
  contenedores `useExtract` y `useSplit` (las dos preguntas al abrir: si fallan, rechaza; un PDF
  de una página se rechaza al abrir Split con la frase del core, como el preflight de Windows);
  vista común `PageRangeDialog` (un campo, pista, rechazo con el diálogo abierto y lo tecleado
  intacto; con puerta cerrada, alerta y solo Cancel) y `ExtractControls`/`SplitControls` (estado,
  **Cancel split** y diálogo). **Split en paralelo con Export images:** una descarga por parte,
  en orden, se para en la primera que falla, al cancelar (tras la parte en curso) o al
  desmontarse, y cuenta lo escrito; todas las partes se piden al motor justo después del plan,
  así salen del mismo estado del documento aunque se edite mientras salen. Sin carpeta ni aviso
  de sobrescritura (los nativos lo tienen; una descarga no sobrescribe). Esperan a la cola de
  ediciones como Save.
- **UX: los botones van en Organize, no en la barra.** **Extract** (activo con selección, abre
  el diálogo con ella: `"2-3"`) y **Split** en la barra de Organize junto a las otras
  herramientas de páginas (Windows también los tiene en su cabecera de Organize; Android en el
  menú del lector). No son ediciones: el permiso de ensamblado no los apaga; se apagan mientras uno de los dos
  corre y, como Undo, mientras una edición de Organize está en curso. **Corrección de revisión
  (R4-001):** la cola de Organize no se entregaba a `useSave` (solo el visor lo hacía), así que
  Save, Compress, Protect, Export, Extract y Split no la esperaban; ahora `useOrganize` la
  registra (`useSaveWaitsFor`), y Extract/Split leen el número de páginas de la última edición
  aplicada (`useDocument.pageCountNow`, fijado en `revise` antes del render), no el del clic. Primero se pusieron en la barra del workspace
  (y en **More tools**), pero dos botones más la partían en otra fila en el host de `embed`
  (botones del host enormes) y bajaban la página fuera de la ventana: los dos tests de
  `embed.spec.ts` que sueltan/dibujan sobre la página fallaban. En lugar de tocar esos tests se
  movieron a Organize; sus líneas de estado y Cancel split sí están en la barra, así sobreviven
  al cambio a View. **More tools no cambia** (Compress, Export images, Protect). Los botones de
  Organize toman el foco al pulsarse (`opensDialog`): Safari no enfoca un botón pulsado y el
  diálogo devolvía el foco a la tarjeta.
- **Parte A (revisión de T-253):** `ui/ToolsMenu.tsx` pasa a `ui/MenuButton.tsx` (componente
  `MenuButton`, clases `menu-button__*`, cabecera genérica: sirve a More tools y a Add blank
  page). Al desactivarse abierto se cierra para siempre (un `useLayoutEffect` lo cierra antes de
  pintar, mientras el foco aún está en el menú) y, si el foco estaba dentro, vuelve al botón;
  volver a activarlo no lo reabre. Doc de `PageEdit` corregido (`insertBlank` usa `at`, no
  `from`/`count`).
- **Tests:** `pdf-save` 1 nativo nuevo (`extraction_refusal` y su orden); `pdf-wasm` 8 nativos
  nuevos (`model/extract.rs`: páginas y sesión intacta, ediciones pendientes, selecciones y
  posición fuera, permiso, una contraseña vs. las dos con copia protegida, firmado, plan y sus
  rechazos) y 5 `wasm-bindgen-test` (`tests/extract.rs`). `@vitela/core` 8 Vitest nuevos (1 de
  `client`, 2 de `handler`, 4 de `documentEngine`, 1 contra pdfium real: `vector_only.pdf` con
  una página borrada sin guardar → 1 página extraída que se abre y pinta, plan con nombres del
  core, corte tras la última rechazado, el borrado aún se deshace). `@vitela/viewer` 22 nuevos (7
  de `extractText`, 10 de `ExtractControls`/`SplitControls`, 2 de `OrganizeScreen`, 1 del
  workspace, 2 de `MenuButton`). Vistos en rojo antes de implementar: los de `MenuButton` (1
  fallando), los de `client`/`handler`/`documentEngine` (7), los de `extractText`, controles
  (sin módulo) y workspace (2) y el de `extraction_refusal` (no compilaba). Los de `pdf-wasm`, el
  de pdfium real y los de `OrganizeScreen` se escribieron junto a su código.
- **E2E (`e2e/extract-split.spec.ts`, 6 × 3):** páginas 2-3 de un PDF de 5 (anchos 300…700) →
  `five-extract.pdf` con exactamente esas dos (`/MediaBox` 400 y 500), que abre con "2 pages",
  y el foco vuelve a Extract; la selección rellena el diálogo y un borrado pendiente está en el
  archivo (500, 600), que sigue sin guardar y deshacible; Split tras 2 y 4 →
  `five-part1..3.pdf` con 2, 2 y 1 páginas; un corte tras la última página se rechaza en el
  diálogo sin descargas; un PDF con `/P` sin el bit 5 (abierto con su contraseña de usuario)
  rechaza en los dos diálogos, con solo Cancel; el diálogo de Split cabe a 390 px. `phone-bar`
  y `zero-network` siguen en verde sin cambios.
- **Verificado en local (Windows):** `cargo fmt --all -- --check`; `cargo clippy --locked -p
  pdf-wasm -p pdf-save --all-targets -- -D warnings` nativo y `cargo clippy --locked --target
  wasm32-unknown-unknown -p pdf-wasm --all-targets -- -D warnings` (con `PDFIUM_WASM_DIR`);
  `cargo check --locked --target wasm32-unknown-unknown -p pdf-wasm`; `cargo test --locked -p
  pdf-wasm` 125 pasan; `cargo test --locked -p pdf-save -- --skip
  export::tests::exports_a_page` 221 pasan (los 2 omitidos necesitan `pdfium.dll` nativo y fallan
  igual sin este cambio); `cargo test -p pdf-ffi --lib -- extract split` pasan. `@vitela/core`:
  `build:wasm`, `typecheck`, Vitest 15 / 197, `build`. `@vitela/viewer`: `tsc -b`, Vitest
  83 / 831, `build`. `apps/web`: `tsc -b`, Vitest 6 / 19, `npm run build`; Playwright completo
  (Chromium, Firefox, WebKit, con `CI=1`) **271 pasan, 20 omitidos** (los de File System Access
  fuera de Chromium), `zero-network`, `phone-bar` y `embed` incluidos.
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; compilan con clippy
  wasm32 y los corre el job `wasm32` de `core.yml`); `linux-gtk`, Android,
  `examples/embed-vite`.
- **No cubierto / pendiente:** Split solo "después de la página N" (el único modo de los
  nativos; "cada N páginas" se escribe como cortes, `2,4`); sin Extract/Split en la vista View
  ni en More tools (están en Organize); sin zip (como Export images); la aceptación de varias
  descargas del navegador no se prueba (Playwright las acepta todas); sin lector de pantalla
  real.

### T-255 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Document properties") y
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API). Lo que condiciona lo
que sigue:

- **Paridad elegida (Android):** diálogo **Document properties** con las seis claves de texto de
  `/Info` (Title, Author, Subject, Keywords, Creator, Producer), con las etiquetas y el orden de
  Android; Windows y Linux muestran las mismas en un panel y además editan las dos fechas, que
  aquí (como en Android) ni se muestran ni se editan: cada cambio las conserva. **Sin XMP:**
  ningún shell sincroniza un flujo `/Metadata` (`pdf_save::metadata` solo escribe `/Info`), así
  que el web tampoco. Puerta: el core no la tiene (`EditClass::of(SetDocumentInfo)` no pide
  nada); los tres nativos preguntan el permiso de modificar contenido
  (`content_editing_is_allowed`), y eso hace `pdf-wasm`.
- **`pdf-wasm`** (`model/metadata.rs`, `engine/metadata.rs`, `dto/metadata.rs`):
  `documentProperties(doc)` → `DocumentPropertiesEntry` (seis `Option<String>`), del último
  `SetDocumentInfo` pendiente o del `/Info` del archivo (`pdf_save::pending_document_info` sobre
  `LopdfDocument::document_info`, como `read_document_info` de `pdf-ffi`); `metadataRefusal(doc)`
  → la frase (`this document does not permit metadata changes`, la de Android) o `undefined`;
  `EditCommand.setDocumentProperties(title, author, subject, keywords, creator, producer)` por
  `applyEdit`: un `Command::SetDocumentInfo { before, after }` (todo el diccionario, un paso de
  deshacer), `""` → clave ausente (regla de Android, ahora en Rust), fechas conservadas, y sin
  paso si no cambia nada. Rechazo → `EditNotPermitted`, sin registrar. El core cuenta el paso
  como visible en la vista previa (`shows_in_session_preview`), así que `@vitela/core` refresca
  (una reapertura con los mismos píxeles); no se tocó esa regla.
- **`@vitela/core`:** `Engine` gana `documentProperties(handle)` y `metadataRefusal(handle)`;
  `Edit` gana `MetadataEdit` (`{ kind: 'setDocumentProperties', properties }`); tipos
  `DocumentProperties` y `MetadataEdit` exportados; `worker/metadata.ts` traduce `null` ↔
  `undefined` y libera el objeto wasm.
- **`@vitela/viewer` (`features/properties/`):** puro con tests `propertiesText.ts` (campos,
  borrador, `propertiesChange`: vacío → `null`, nada nuevo → `null`); contenedor
  `useDocumentProperties` (lee valores y puerta al abrir; **Apply** espera la cola de
  ediciones, relee — un deshacer pudo llegar, como Android relee bajo su lane — y solo registra
  un cambio real; informa con `revise` y marca sucio; un rechazo deja el diálogo y lo tecleado);
  vistas `PropertiesDialog`, `PropertiesControls` (estado + diálogo, en la barra) y
  `PropertiesToggle`. **`useSave`** ahora espera **varias** colas (`waitForEdits` guarda un
  `Set`): la pantalla y el Apply del diálogo; antes la última registrada reemplazaba a la otra.
- **UX: dónde va Properties.** Primero se puso un botón en la barra del workspace: en el host
  de `embed` (botones del host enormes) la barra ya ocupa 1191 de 1200 px y el botón la partía
  en dos filas (falló `embed.spec.ts` "undoes a stamp dropped…"). Después se probó convertir la
  línea del documento ("report.pdf — 1 page") en el botón (cero ancho): rompía la premisa de
  `embed.spec.ts` (esa línea no toma el foco) y chocaba con búsquedas por rol (`copy.pdf` contra
  el botón Copy de `text.spec.ts`). Luego, en la fila **Annotations** del visor junto a
  **Comments** y **Forms** (donde Windows y Linux tienen el panel): a 1280 px esa fila dejaba de
  caber junto a Draw e History, aparecía una fila más de barras y el trazo de
  `draw.spec.ts` salía del viewport en Firefox. Queda **al final de la barra View** del visor
  (zoom y página): crece para llenar la fila que comparte con Text (~144 px libres en la app,
  ~770 en el host; el botón ocupa ~100), así no añade filas; `properties.spec.ts` lo fija
  (View con Text y Annotations con Draw en la misma fila). En el móvil, oculto (≤ 600 px) y en
  **More tools**. El diálogo y su estado viven en la barra; el foco vuelve al botón, o a More
  tools si no se ve. En escritorio no hay acceso desde Organize.
- **Parte A (revisión de T-254):** (1) `useSplit` pide al motor como mucho **una parte por
  delante** de la que se escribe (como Export images, página a página): Cancel para también el
  trabajo y solo hay dos partes en memoria. Contrapartida: ya no salen todas del mismo estado;
  una edición hecha mientras corre llega a las partes pedidas después (la cabecera lo dice).
  (2) `useOrganize.settledNow` repite mientras haya un paso en curso o uno esperando turno
  (contador `waiting` de `runInTurn`), así Save espera también la importación encolada.
- **Tests:** `pdf-wasm` 6 nativos nuevos (`model/metadata.rs`: lectura y claves ausentes, un paso
  deshecho/rehecho antes de guardar, guardado con la fecha de creación conservada y la clave
  vaciada borrada, sin cambio no hay paso, permiso denegado de solo lectura, historial mezclado
  con páginas) y 2 `wasm-bindgen-test` (`tests/metadata.rs`). `@vitela/core` 6 Vitest nuevos (1
  de `client`, 1 de `handler`, 3 de `documentEngine`, 1 contra pdfium real: cambiar, deshacer,
  rehacer, guardar y releer). `@vitela/viewer` 18 nuevos (1 de `useSplit`, 1 de `useOrganize`, 1
  de `useSave`, 5 de `propertiesText`, 9 de `PropertiesControls`, 1 del workspace). Vistos en rojo
  antes de implementar: los de Parte A (parte 3 pedida; Save no esperaba la importación), el de
  `useSave`, los de `client`/`handler`/`documentEngine` (5) y los de `PropertiesControls` (sin
  módulo; y otra vez al cambiar el disparador). Los de `pdf-wasm`, `propertiesText`, pdfium real
  y el del workspace se escribieron junto a su código.
- **E2E (`e2e/properties.spec.ts`, 4 × 3):** título y autor (`Ana Peña`) aplicados, sin guardar,
  deshechos y rehechos (el diálogo muestra cada estado), guardados: el `/Info` del archivo los
  tiene y no tiene `/Subject` (helper nuevo `documentInfo` en `e2e/pdfObjects.ts`: sigue la
  referencia `/Info` del tráiler, objeto suelto o dentro de un `/ObjStm`, y decodifica literales
  y UTF-16BE); reabierto, el diálogo los muestra y un Apply sin cambios no registra nada; un PDF
  con `/P` sin el bit 4 (abierto con su contraseña de usuario) es de solo lectura con solo Close;
  en el móvil se abre desde More tools, cabe en 390 px y el foco vuelve a More tools; a 1280 px
  el botón no crea una fila de barras nueva.
  `phone-bar.spec.ts` lista Properties en More tools.
- **Verificado en local (Windows):** `cargo fmt --all -- --check`; `cargo clippy --locked -p
  pdf-wasm --all-targets -- -D warnings` nativo y `cargo clippy --locked --target
  wasm32-unknown-unknown -p pdf-wasm --all-targets -- -D warnings` (con `PDFIUM_WASM_DIR`); `cargo
  test --locked -p pdf-wasm` 131 pasan. `@vitela/core`: `build:wasm`, `typecheck`, Vitest 15 /
  203, `build`. `@vitela/viewer`: `tsc -b`, Vitest 85 / 851, `build`. `apps/web`: `tsc -b`,
  Vitest 6 / 19, `npm run build`; Playwright completo (Chromium, Firefox, WebKit, con `CI=1`)
  **283 pasan, 20 omitidos** (los de File System Access fuera de Chromium), `zero-network`, `phone-bar` y `embed` incluidos.
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; compilan con clippy
  wasm32 y los corre el job `wasm32` de `core.yml`); `linux-gtk`, Android,
  `examples/embed-vite`.
- **No cubierto / pendiente:** sin fechas (Windows y Linux las editan); sin acceso a Properties
  desde Organize en escritorio; sin claves personalizadas ni `/Trapped` (fuera de alcance del
  core, decisión 2 del batch de metadatos); sin lector de pantalla real.

### T-256 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Tools menu" y "Watermark") y en
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API). Lo que condiciona lo
que sigue:

- **Parte A — menú Tools (decisión del orquestador).** La barra del workspace estaba llena (en el
  host de `embed`, 1191 de 1200 px; Properties tuvo que irse a la barra View en T-255). Los botones
  Compress, Export images y Protect de la barra y el Properties de la barra View desaparecen: hay
  **un** botón de menú **Tools** (`ui/MenuButton`) en la barra, antes de Save, en las dos pantallas
  y a **todos** los anchos, con Compress, Export images, Protect, Watermark y Document properties.
  El "More tools" del teléfono es este mismo menú (un solo camino de código); en ≤ 600 px muestra
  los tres puntos en vez de la palabra (su nombre accesible sigue siendo "Tools") y la barra sigue en
  una fila. Cada `*Controls` ya no dibuja botón: recibe `trigger` (el botón Tools) y le devuelve el
  foco al cerrarse su diálogo (`ui/focusShown`); el menú devuelve el foco al botón antes de abrir el
  diálogo, como antes. Las líneas de estado y los diálogos siguen en la barra (sobreviven al cambio
  View/Organize). Se borró `PropertiesToggle.tsx` y `ZoomControls` vuelve a no aceptar hijos. Save y
  las líneas de estado no cambian.
- **Paridad elegida (Android, con los rangos de GTK y Windows):** diálogo **Add watermark** con
  Text/Picture, el texto (DRAFT) o un PNG/JPEG (**Choose picture…**), Size (8–200 pt o 5–100 % de la
  página), Opacity (5–100 %), Angle (−180…180°), Placement (sobre o bajo el contenido) y Pages
  (todas, la actual o un rango con la gramática del core). Cambiar de tipo solo reinicia ángulo y
  capa a los del core para ese tipo (regla de los tres nativos). **Sin color ni vista previa en
  vivo:** ningún nativo los tiene. El aspecto inicial es el del core (`Watermark::diagonal` /
  `Watermark::picture`), nunca una copia de sus constantes.
- **`pdf-wasm`** (`model/watermark.rs`, `engine/watermark.rs`, `dto/watermark.rs`): mismo orden que
  `pdf-ffi` para `AddWatermark`/`AddImageWatermark`: `pdf_save::check_edit` con la clase de contenido
  (permiso de modificar, luego si la reescritura completa puede conservar el cifrado) **antes** de
  leer las páginas; posiciones → `PageId` una sola vez (el sello sigue a sus páginas tras mover);
  `pdf_save::validate_content_command` (= `pdf_edit::check_watermark`) antes de registrar
  (`EncodingGap`, `InvalidImage`); el modelo rechaza texto en blanco, páginas repetidas o fuera de
  rango (`UnsupportedOperation`). Superficie: `watermarkDefaults()` (función libre →
  `WatermarkLook`), `watermarkRefusal(doc)`, `EditCommand.addTextWatermark(pages, text, fontSize,
  opacity, rotationDegrees, behind)` y `addPictureWatermark(pages, image, scale, …)` por `applyEdit`.
  Nueva dependencia de ruta `pdf-edit` (ya estaba en el árbol vía `pdf-save`) solo para los tipos de
  rechazo.
- **Repintado en el sitio (`paintRevision`).** Un sello cambia lo que muestra una página sin cambiar
  su id ni su giro, que es la clave de los bitmaps del visor y de la caché de miniaturas de Organize:
  sin esto, la página seguía mostrando los píxeles viejos (visto en rojo en Playwright). Las marcas de
  vista previa de `pdf-wasm` (`model/preview.rs`) ganan una segunda marca por paso de contenido
  (`Command::is_content_edit`); `paintRevision(doc)` es la más nueva aplicada (0 = el archivo). En
  una historia lineal esa marca nombra todo el conjunto aplicado: deshacer vuelve a la revisión
  anterior (sus bitmaps vuelven a valer) y un paso nuevo en lugar de uno deshecho tiene la suya.
  `EditedDocument.paintRevision` (opcional en el tipo: un motor que no la lleve cuenta como 0) entra
  en el `paintKey` del visor y en `thumbnailKey`; un movimiento, un giro o una anotación siguen sin
  repintar nada (la regla "un movimiento no renderiza miniaturas" se conserva).
- **`@vitela/core`:** `Engine` gana `watermarkDefaults()` y `watermarkRefusal(handle)`; `Edit` gana
  `WatermarkEdit` (`{ kind: 'addWatermark', pages, mark, opacity, rotationDegrees, behind }`, `mark`
  texto o imagen); `worker/watermark.ts` traduce y libera. Hallazgo: en `reportlab_acroform.pdf` el
  sello falla en el refresco de la vista previa (`SaveFailed`: flujo de contenido con cadena de
  filtros, límite del core igual en los nativos) y `step` lo deshace; queda un paso rehacible que
  volvería a fallar igual (comportamiento previo de `step` para cualquier edición, no se tocó).
- **`@vitela/viewer` (`features/watermark/`):** puro con tests `watermarkText.ts` (borrador desde el
  aspecto del core, rangos, cambio de tipo, rechazos de marca, la edición, textos); contenedor
  `useWatermark` (lee aspecto y puerta al abrir; **Add** espera la cola, resuelve páginas contra
  `pageCountNow` y la página actual recortada, registra una edición, `revise` + sucio; un rechazo deja
  el diálogo y el borrador; se entrega a `useSave`); vistas `WatermarkDialog` y `WatermarkControls`.
  El campo de texto se llama "Watermark text": "Text" chocaba con la radio Text.
- **Tests:** `pdf-wasm` 8 nativos nuevos (6 de `model/watermark.rs`: defaults del core, texto en todas
  las páginas como un paso deshecho/rehecho y guardado, el sello sigue a su página tras mover,
  imagen, rechazos sin registrar, permiso antes que la página; 2 de `model/preview.rs`) y 3
  `wasm-bindgen-test` (`tests/watermark.rs`). `@vitela/core` 6 Vitest nuevos (1 de `client`, 1 de
  `handler`, 3 de `documentEngine`, 1 contra pdfium real: sello en `vector_only.pdf`, píxeles,
  búsqueda, deshacer/rehacer, guardado, `EncodingGap`, y el fallo de refresco deshecho en el
  formulario). `@vitela/viewer` 18 nuevos (6 de `watermarkText`, 9 de `WatermarkControls`, 1 del
  workspace, 1 de `thumbnailKey`, 1 de `useDocument`) y uno quitado (el de `PropertiesToggle`);
  los de Compress/Export/Protect/Properties abren ahora desde un disparador que hace de entrada del
  menú y comprueban que el foco vuelve a él. Vistos en rojo antes de implementar: los de
  `client`/`handler`/`documentEngine` (5), los de `thumbnailKey` y `useDocument` (2) y el E2E del
  texto (la página no se repintaba). Los 5 del workspace fallaron al quitar los botones y se
  actualizaron después. Los de `pdf-wasm`, pdfium real, `watermarkText` y `WatermarkControls` se
  escribieron junto a su código.
- **E2E (`e2e/watermark.spec.ts`, 6 × 3):** texto en las 3 páginas (foco a Tools, sin guardar, la
  tinta de la página cambia, Undo la quita, Redo la devuelve), guardado: cada página tiene el `Tj` y
  un artefacto `/Watermark` (helper nuevo `occurrences` en `e2e/pdfObjects.ts`), y la búsqueda de la
  app lo encuentra 3 veces al reabrir; imagen detrás del contenido de `1,3`: un solo objeto `/Image`
  pintado dos veces (y Add sin imagen se rechaza); texto que Helvetica no muestra y un rango fuera
  dejan el diálogo; un PDF con `/P` sin el bit 4 (abierto con su contraseña de usuario) se rechaza con
  solo Close; en Organize las miniaturas ganan el sello y lo pierden con Undo; en el móvil el diálogo
  cabe en 390 px. `phone-bar`, `compress`, `export-images`, `protect` y `properties` abren las
  herramientas por el menú (helpers `toolsButton`/`openTool` en `e2e/support.ts`); `phone-bar` ahora
  exige que ninguna herramienta tenga botón propio y que se vean los tres puntos, y `properties`
  que la barra sea una fila a 1280 px.
- **Verificado en local (Windows):** `cargo fmt --all -- --check`; `cargo clippy --locked -p pdf-wasm
  --all-targets -- -D warnings` nativo y `cargo clippy --locked --target wasm32-unknown-unknown -p
  pdf-wasm --all-targets -- -D warnings` (con `PDFIUM_WASM_DIR`); `cargo test --locked -p pdf-wasm`
  139 pasan. `@vitela/core`: `build:wasm`, `typecheck`, Vitest 15 / 209, `build`. `@vitela/viewer`:
  `tsc -b`, Vitest 87 / 868, `build`. `apps/web`: `typecheck` (`tsc -b`), Vitest 6 / 19, `npm run
  build`; Playwright completo (Chromium, Firefox, WebKit, con `CI=1`) **301 pasan, 20 omitidos** (los
  de File System Access fuera de Chromium), `zero-network`, `phone-bar` y `embed` incluidos.
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; compilan con clippy wasm32 y
  los corre el job `wasm32` de `core.yml`); `linux-gtk`, Android, `examples/embed-vite`.
- **No cubierto / pendiente:** sin color ni vista previa en vivo (ningún nativo); un PDF cuyo flujo
  de contenido el core no reescribe (cadena de filtros) rechaza el sello al refrescar (`SaveFailed`)
  con el mensaje del core; el paso rehacible que deja ese rechazo es de `step`, no de esta tarea; sin
  lector de pantalla real.

### T-257 — resultado

Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Print"),
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API) y
[`packages/vitela-viewer/README.md`](../packages/vitela-viewer/README.md) ("Keys and drops"). Lo
que condiciona lo que sigue:

- **Parte A — tope de la imagen de la marca de agua (revisión de T-256).** `pickPicture` de
  `useWatermark` leía el archivo entero sin límite. Ahora reutiliza `MAX_STAMP_BYTES` (20 MB, la
  constante de los sellos de imagen de T-239, `features/annotations/stamps.ts`): un archivo más
  grande se rechaza **sin leerlo** con "Pictures larger than 20 MB cannot be stamped on the pages.
  Choose a smaller one." y el borrador conserva la imagen anterior. 1 Vitest nuevo en
  `WatermarkControls.test.tsx` (visto en rojo antes del cambio).
- **Qué se imprime: la instantánea de salida, como GTK y Windows** (imprimen la sesión anotada desde
  su `output_snapshot`). **Print** espera a la cola de ediciones y abre `openPrintSnapshot`: lo que
  escribiría un guardado ahora, así que entran las ediciones de página, anotaciones, rellenos y marcas
  de agua sin guardar. No es un paso: ni historial ni refresco.
- **Sin puerta de permiso (decisión, no hallazgo).** En el core **no existe** regla para el bit de
  impresión de `/P` (`pdf-manip/src/security.rs` interpreta extracción, anotar, modificar y
  ensamblar; nada más), y ningún nativo pregunta antes de su diálogo de impresión (GTK
  `print_document`, Windows `PreparePrintAsync`, Android `PdfPrintDocumentAdapter`). Por eso la web
  no inventa una: un PDF con `/P` sin el bit 3 imprime, y el E2E lo fija. `openImageExport` sí exige
  el permiso de extracción, así que la impresión **no** lo reutiliza: `pdf-wasm` gana
  `openPrintSnapshot` (la misma instantánea sin esa puerta, `DocumentModel::print_snapshot_bytes`;
  `output_snapshot_bytes` = puerta + esa función). Si se quiere honrar el bit de impresión, es una
  regla nueva del core para todos los shells, no de la web.
- **`pdf-wasm`:** `Engine.openPrintSnapshot(doc)` → `OpenedImageExport` en el mismo mapa que las
  exportaciones: las páginas salen con `exportPageImage` y se cierra con `closeImageExport`.
  `open_export` y la impresión comparten `open_snapshot`. 2 tests nativos (`model/export.rs`: la
  anotación pendiente entra y no se registra nada; un documento sin extracción imprime) y 1
  `wasm-bindgen-test` (`tests/export.rs`). Los nativos se vieron en rojo.
- **`@vitela/core`:** `Engine.openPrintSnapshot(handle)` (`client`, `protocol`, `handler`,
  `documentEngine`). 2 Vitest nuevos (`documentEngine`: un documento restringido imprime y se cierra
  como una exportación, documento cerrado → `DocumentNotFound`; pdfium real: el resaltado sin
  guardar cambia los píxeles, nada que deshacer) y 2 ampliados (`client`, `handler`). Vistos en rojo.
- **`@vitela/viewer` (`features/print/`):** puros con tests `printSheets.ts` (`PRINT_DPI` = 150,
  `pngSize` del IHDR, `sheetOf` en puntos desde los píxeles, `printStyles` con un `@page` con nombre
  por tamaño distinto, `writePrintDocument`) y `printText.ts` (`printKey`: ⌘+P en Apple, Ctrl+P en el
  resto, sin otros modificadores; frases). Adaptador `printFrame.ts` (`iframeSurface`); contenedor
  `usePrint`; `usePrintKey`; vista `PrintControls` (estado + **Cancel print**). En el menú **Tools**
  tras Export images. Flujo: cola → instantánea → techo de raster a 150 DPI para todas las páginas
  (rechazo "Page N is too large to print." antes de dibujar) → una página cada vez a PNG por el core,
  guardando solo el archivo como blob URL ("Preparing page 2 of 5 for printing…"; Cancel para tras
  la página en curso) → la instantánea se cierra **antes** del diálogo, acabe como acabe → marco →
  `print()`. 150 DPI y no los 300 de GTK: la web retiene cada página como imagen hasta que se cierra
  el diálogo (Letter ≈ 2 Mpx en vez de 8,4).
- **El marco:** `iframe` oculto (`srcdoc`: Firefox sustituye el primer documento de un marco sin
  `src` al cargar; 0 × 0 y `visibility: hidden`, no `display: none`, que en Firefox no imprime)
  **dentro de la raíz del editor**, nunca en otro sitio de la página anfitriona; imprime su propia
  ventana, así que la página de alrededor nunca se imprime y ningún estilo cruza (el marco tiene un
  solo `<style>` propio). Cada página es una hoja del tamaño de su imagen en puntos, con un `@page`
  con nombre por tamaño: la página girada sale apaisada. El título es el nombre sin `.pdf` (lo que el
  diálogo propone al guardar en archivo). El marco y sus blobs se van con `afterprint`, o con la
  siguiente impresión, un documento nuevo o el desmontaje (si un navegador no avisa, la siguiente
  impresión lo limpia).
- **Ctrl/⌘+P** con el alcance de T-246 (`EditorScope.ownsEvent`): con el foco en el editor, o sin
  foco tras el último clic/soltado en él; en el anfitrión, la tecla apuntada a la página anfitriona
  se deja al navegador. Tampoco con un diálogo modal del editor abierto, ni si otro manejador ya la
  tomó.
- **Tests:** `@vitela/viewer` 16 nuevos (4 de `printSheets`, 2 de `printText`, 9 de
  `PrintControls`: orden cola/instantánea/techo/dibujo/cierre/marco/print, hoja apaisada,
  `afterprint` libera marco y blobs, Cancel, techo, fallo de página, fallo de apertura, una segunda
  impresión limpia la anterior y un `afterprint` tardío no toca la nueva, desmontaje sin marco, la
  tecla dentro y fuera/con diálogo; 1 de la marca de agua) y el del menú del workspace actualizado.
  `printSheets`/`printText` y el test del menú se vieron en rojo; el hook y su test se escribieron
  juntos, y se comprobó que el test cae al quitar la limpieza previa y la condición del diálogo.
- **E2E (`e2e/print.spec.ts`, 6 × 3, uno solo en Chromium):** un script de inicio sustituye el
  `print` del marco (y el de la página) y anota lo que tenía el marco. Tres páginas → tres imágenes
  en orden, 1275 × 1650 y la girada 1650 × 1275, en hojas `612pt 792pt` / `792pt 612pt`, con tinta,
  marco `aria-hidden` y `tabindex=-1`, y `afterprint` lo quita; **Chromium:** el documento del marco
  impreso a PDF (`page.pdf`, `preferCSSPageSize`) tiene esas dos `MediaBox`; una marca de agua sin
  guardar añade tinta a la hoja y el documento sigue sin guardar; Ctrl+P con el foco en la app
  imprime el marco y nunca la app; un PDF con `/P` sin imprimir ni copiar imprime; **embed:** Ctrl+P
  con el foco en un botón del anfitrión no imprime nada del editor, y en el editor imprime una imagen
  desde un marco dentro de `#host-frame` con una sola hoja de estilo y `margin: 0`. `phone-bar`
  espera Print en el menú.
- **Verificado en local (Windows):** `cargo fmt --all -- --check`; `cargo clippy --locked -p pdf-wasm
  --all-targets -- -D warnings` nativo y `cargo clippy --locked --target wasm32-unknown-unknown -p
  pdf-wasm --all-targets -- -D warnings` (con `PDFIUM_WASM_DIR`); `cargo test --locked -p pdf-wasm`
  141 pasan. `@vitela/core`: `build:wasm`, `typecheck`, Vitest 15 / 211, `build`. `@vitela/viewer`:
  `tsc -b`, Vitest 90 / 884, `build`. `apps/web`: `typecheck` (`tsc -b`), Vitest 6 / 19, `npm run
  build`; Playwright completo (Chromium, Firefox, WebKit, con `CI=1`) **316 pasan, 1 falla, 22 omitidos** (los de File System Access fuera de Chromium y el de `page.pdf` fuera de Chromium). El fallo fue `compress.spec.ts:57` en WebKit (timeout esperando que la primera tarjeta de Organize quede estable, nada de impresión); repetido aislado `--repeat-each=5` pasa 20 / 20, y una pasada completa anterior dio 317 pasan y 0 fallos por test (ese proceso no llegó a imprimir el resumen: quedó colgado al cerrar los servidores de `preview`), `zero-network`,
  `phone-bar` y `embed` incluidos.
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; compilan con clippy wasm32);
  `linux-gtk`, Android, `examples/embed-vite`.
- **No cubierto / pendiente:** el diálogo real de impresión de cada navegador (el E2E lo sustituye) y
  el papel físico; Safari puede ignorar los `@page` con nombre (la imagen conserva proporción con
  `object-fit: contain`); sin vista previa propia ni elección de páginas (son del diálogo del
  navegador); un anfitrión con CSP `style-src` estricta bloquearía el `<style>` del marco (la app no
  tiene CSP); el bit de impresión de `/P` no se honra (ver arriba).
- **Revisión de T-257 (rama `fix/web-print-review`).**
  - **Cancel print** se oía solo antes de cada página. Ahora también tras la última y mientras el
    marco decodifica (`decodeInOrder` corta, y `usePrint` mira la parada antes de `job.print()`):
    hasta que se abre el diálogo, cancelar = sin diálogo, marco quitado, blobs revocados.
  - **Memoria: DPI adaptativo (elegido frente a rechazar).** `printDpi`: 150 DPI mientras todas las
    páginas sumen ≤ 128 Mpx (`PRINT_PIXEL_BUDGET`; el marco las retiene decodificadas, ~512 MB, ≈ 60
    Letter a 150), si no la mayor que cabe, con suelo de 96 (`PRINT_MIN_DPI`, ≈ 148 Letter). Por
    encima del suelo **se sigue imprimiendo** a 96: el usuario espera que imprimir funcione, y
    rechazar dejaría sin imprimir documentos de 200 páginas; la memoria crece entonces ~3,4 MB
    decodificados por Letter, como en la impresión del propio navegador. Las páginas sin tamaño
    conocido (instantánea con una edición pendiente) cuentan como la media. La decodificación es
    secuencial (antes `Promise.all`). El techo de raster se pregunta al DPI elegido.
  - **Duplicación:** `features/snapshot/` — `snapshotJob.ts` (`withSnapshot`: cola → abrir →
    trabajo → cerrar pase lo que pase; `drawPages`: una página cada vez, parada antes de cada una,
    `PageNotDrawn`) y `useSnapshotJob` (uno a la vez, descartado al cambiar de documento o
    desmontar, `stop`). Lo usan `useExportImages` (entrega por página y su Cancel, sin cambios de
    comportamiento) y `usePrint` (el marco).
  - **`openPrintSnapshot` solo para imprimir:** documentado en `types.ts` y el README del core; en
    `pdf-wasm` cada instantánea recuerda su uso (`SnapshotUse::{Export, Print}`) y
    `exportPageImage` sobre una de impresión rechaza más de `PRINT_MAX_DPI` (150) con
    `UnsupportedOperation`: no es una vía para saltarse el permiso de extracción. Renombrado:
    `export_snapshot_bytes` (puerta + `snapshot_bytes`) y `print_snapshot_bytes` (=
    `snapshot_bytes`), así la llamada se lee hacia delante; `output_snapshot_bytes` ya no existe.
  - **Menores:** fuera `printShortcut` (sin uso); orden de imports en `WatermarkControls.test.tsx`.
  - **Tests:** Rust 1 nativo (`SnapshotUse::check_dpi`, visto en rojo) + 1 `wasm-bindgen-test`
    (no corrido en local); `@vitela/core` 1 Vitest pdfium real (150 pasa, 151 rechaza, export a 300
    pasa; escrito con el wasm ya recompilado, no visto en rojo); `@vitela/viewer` 23 nuevos
    (`snapshotJob` 9, `useSnapshotJob` 5, `printDpi` 4, `decodeInOrder` 2, `PrintControls` 3:
    cancelar en la última página, cancelar mientras se monta el marco, documento largo a menos DPI),
    todos vistos en rojo antes de implementar.
  - **Verificado en local (Windows):** `cargo fmt --all -- --check`; clippy `pdf-wasm` nativo y
    wasm32 (`--all-targets -D warnings`, con `PDFIUM_WASM_DIR`); `cargo test --locked -p pdf-wasm`
    142 pasan. `@vitela/core`: `build:wasm`, `typecheck`, Vitest 15 / 212, `build`.
    `@vitela/viewer`: `tsc -b`, Vitest 93 / 907, `build`. `apps/web`: `typecheck`, Vitest 6 / 19,
    `build`. Playwright (`CI=1`, 3 navegadores) print + export-images + phone-bar + embed +
    zero-network **70 pasan, 2 omitidos**; suite completa **317 pasan, 22 omitidos, 0 fallos**.
    No corridos: los `wasm-bindgen-test`.

### T-258 — resultado

**T-258a** (#666, sin fusionar al empezar T-258b; su commit se tomó con `cherry-pick`): la
orquestación de la edición de contenido que solo vivía en `pdf-ffi` pasó a
`pdf_save::content_edit` (gates, `record_content`, comprobación de reemplazo de imagen) y
`pdf_edit::group_text_paragraphs`; `pdf-ffi` delega. **T-258b** la usa desde `pdf-wasm`.
Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Edit content") y en
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API). Lo que condiciona lo
que sigue:

- **`CheckpointSession` en `pdf-wasm`** (`model/checkpoint.rs`, espejo de
  `pdf-ffi/src/document/checkpoint.rs`): `DocumentModel` gana `checkpoints`
  (`pdf_save::Checkpoints<(LopdfDocument, Vec<u8>)>`) y `has_password` (si la apertura llevó
  contraseña); `original_bytes` es siempre `Some` (la web abre siempre desde bytes);
  `has_imported_sources` = el registro de importados no está vacío; `replace_base` cambia base y
  bytes. `undo`/`redo`/`canUndo`/`canRedo` pasan por `pdf_save::checkpoint::{undo, redo,
  can_undo, can_redo}`: deshacer cruza un checkpoint paso a paso. Todas las ediciones de página,
  anotación, formulario, metadatos y marca de agua siguen igual (sus tests, sin tocar, en verde).
  `DocumentModel::record_content(command)` (aún sin UI: lo usarán T-259+) = `ensure_content_editing`
  + `pdf_save::content_edit::record_content` con `record_content_command` o, para un
  `Command::Batch`, `record_content_batch`. Nuevos `From<RecordError>` y
  `From<ContentEditRefusal>` para `EngineError` (`EncodingGap`/`InvalidImage` conservan su tipo;
  un permiso, `EditNotPermitted`; "guarda primero", `UnsupportedOperation`).
- **`PreviewMarks` y la edición de contenido** (`model/preview.rs`). Dos casos nuevos:
  - **Plegado (amend):** `record_content_command` puede reescribir una entrada ya encolada (segundo
    retipeo de la misma línea, segundo movimiento de la misma imagen) sin añadir paso, y **no
    siempre la última**: el segundo movimiento de una línea pliega en su movimiento aunque después
    haya otra edición. `record_content` compara la cola antes y después, localiza la primera
    entrada cambiada y `PreviewMarks::amended(n)` da marcas nuevas (vista previa y repintado) a
    ese paso y a todos los posteriores, y vacía el lado de rehacer como hace `EditLog::amend`.
    Dar marca nueva solo al último paso (lo que pedía la ficha) dejaría, tras deshacer el último,
    una revisión de pintado que nombra píxeles anteriores al plegado: bitmaps viejos en caché.
  - **Checkpoint:** si la generación de base cambió, el reintento es **un** paso nuevo
    (`recorded`). Las marcas siguen siendo **una pila plana** a través de las bases, sin marco por
    generación: cada deshacer/rehacer a través de un checkpoint es exactamente un paso del
    historial del lector y el checkpoint no cambia cuántos pasos hay ni qué muestra cada uno, así
    que la pila plana coincide paso a paso con la historia (lo comprueban los tests de deshacer y
    rehacer a través del checkpoint). Desviación consciente de "push/pop a frame".
- **Lecturas** (`model/content.rs`, `engine/content.rs`, `dto/content.rs`), con las puertas de
  `pdf-ffi`: todas detrás del permiso de extracción (antes que la página), con los lectores que
  superponen lo pendiente (`read_pending_page_content_of`, `text_run_style_of`,
  `page_font_families_of`). Superficie: `contentEditingAllowed(doc)` =
  `pdf_save::content_editing_allowed`; `contentEditingRefusal(doc)` = la frase de `check_edit`
  para una edición de contenido y, si no, la del permiso de texto (un editor que no puede leer lo
  que edita tampoco se ofrece); `readPageContent(doc, page)` → `PageContentEntry` (`takeRuns`,
  `takeImages`, `takePaths`): runs con `id`, `bbox`, `fontName` (recurso), `fontKind`
  (`standard14`/`embeddedSimple`/`embeddedComposite`), `text` y `editable`; imágenes con `inline`;
  trazados con `paint`; `readTextBlocks`/`readParagraphBlocks` (`group_text_lines` /
  `group_text_paragraphs`, miembros por id como `pdf-ffi`); `textRunStyle(doc, page, runId)` (por
  id sobre la lectura pendiente: base font, tamaño, color, espaciado, escala, negrita, cursiva,
  modo; `undefined` para un run que solo existe por una inserción pendiente; id desconocido →
  `UnsupportedOperation`); `pageFontFamilies` (pares `[recurso, baseFont, …]`) y
  `contentBaseGeneration` (también en `EditedDocument`). Los ids cruzan como `number`: el core los
  mantiene < 2^53 (sintéticos `2^40 + índice + (parte << 24)`).
- **`editable` es regla del core:** `pdf_save::text_run_text_is_decoded` (nuevo, en
  `content_edit.rs`): un run cuyo texto lleva U+FFFD (código sin `/ToUnicode` utilizable) no se
  puede retipear (el codificador rechazaría U+FFFD con `EncodingGap`). Ningún shell nativo lo
  pregunta hoy.
- **Tamaño y color no van en cada run:** `text_run_style` recorre el flujo de la página por cada
  run (O(n²) por página); van en `textRunStyle`, que se pide para el run elegido.
- **`@vitela/core`** (`worker/content.ts`): las entradas wasm pasan a valores planos con las cajas
  en la página **dibujada** (`PageFrame.rectFromPdf`, como todas las del motor) y se liberan;
  color 0–255. `Engine`: `contentEditingRefusal`, `readPageContent`, `readTextBlocks`,
  `readParagraphBlocks`, `textRunStyle`, `pageFontFamilies`, `contentBaseGeneration`; tipos
  `PageContent`, `ContentItem` (`ContentRun`/`ContentImage`/`ContentPath`), `ContentFontKind`,
  `TextBlock`, `TextLine`, `TextRunStyle`. `textRunLook` (negrita/cursiva) queda para T-259 (los
  botones de estilo).
- **`@vitela/viewer` (`features/content/`):** puro con tests `contentModel.ts`
  (`pagesToParse`: visibles ± 1, nunca el documento; `contentKey(pageId, paintRevision,
  generation)`; `keepWanted`: solo se guardan las claves aún deseadas; `itemAt`: el elemento más
  pequeño bajo el puntero, con 3 pt de holgura para reglas de alto 0; textos), contenedor
  `useContentMode` (puerta una vez por documento; activo solo para el documento en que se activó;
  lecturas que solo aterrizan en su clave si sigue esperándolas; hover y selección que caducan si
  la página cambia de clave; mensaje de la página que no se pudo leer), vistas `ContentLayer`
  (capa encima de todo en cada página: contornos por tipo — run sólido azul, imagen discontinuo
  ámbar, trazado punteado verde — y la pulsación) y `ContentStatus` (fila de estado con **Done**).
  Con el modo activo el texto, las anotaciones y el dibujo sueltan el puntero (herramienta a
  Select, selección de texto y de anotación fuera); Escape en las páginas suelta la selección.
- **Dónde va el interruptor: último elemento de Tools**, un `menuitemcheckbox` ("Edit content",
  `aria-checked`; desde Organize cambia al visor). La puerta (`useContentGate`) se pregunta en el
  workspace una vez por documento; rechazado, el elemento sigue en la lista `aria-disabled` y
  descrito (`aria-describedby`) por la frase del core. `ui/MenuButton` gana `checked` y
  `description` en `MenuTool`. Las barras del visor no tienen sitio (medido en Playwright): tras
  History, a 1280 px Annotations bajaba a una tercera fila (`properties.spec.ts`, faltaban ~8 px);
  tras View y Text cabía con 22 px de holgura, pero "Page 120 of 120" la desbordaba
  (`comments.spec.ts`, las páginas se movían); y a 390 px una fila más tapaba la cabecera del panel
  de comentarios (`comments.spec.ts:192`). La barra del workspace no cambia (una fila en `embed` y
  a 390 px, `phone-bar` en verde: un `menuitemcheckbox` no cambia la lista de `menuitem` que
  comprueba). Con el modo activo, una fila bajo las barras lleva el estado y **Done** (devuelve el
  foco a Tools); Organize o abrir otro documento también lo apagan.
- **Tests:** Rust 18 nativos nuevos (15 de `model/content`: los tres tipos, `editable`, puertas y
  su orden, líneas y párrafos, estilo y fuentes, retipeo pendiente, plegado que repinta, plegado
  de un paso antiguo, checkpoint como un paso con deshacer/rehacer a través, guardado tras
  checkpoint, edición de página tras contenido, rechazo sin permiso; 2 de `model/preview`; 1 de
  `pdf-save` `text_run_text_is_decoded`) y 2 `wasm-bindgen-test` (`tests/content.rs`).
  `@vitela/core` 7 Vitest nuevos (1 `client`, 1 `handler`, 4 `documentEngine`, 1 pdfium real:
  `reportlab_quote_paragraph.pdf` con acentos por `/ToUnicode`, párrafo de 4 líneas a 11 pt,
  estilo Vera 14, fuentes, y la imagen del escaneo). `@vitela/viewer` 22 nuevos (9 de
  `contentModel`, 2 de `useContentGate`, 5 de `useContentMode`, 2 de `ContentStatus`, 2 de
  `MenuButton`, 2 del workspace: encender/apagar desde Tools en las dos pantallas, Done y Organize,
  y el elemento rechazado con su frase). Vistos en rojo antes de
  implementar: los de plegado (quitando `amended`, 2 caen). El resto se escribió junto a su código.
- **E2E (`e2e/edit-content.spec.ts`, 4 × 3):** run bajo el puntero contorneado alrededor del
  puntero, pulsación → estado "Text “Vitela content line” selected on page 1.", sin selección de
  texto, Escape suelta, el elemento de Tools se lee marcado y Done apaga con el foco en Tools; alejado al mínimo en 30 páginas solo se leen las visibles y la
  siguiente, y toda página montada más lejos queda `waiting`; el escaneo contornea y selecciona
  una imagen; un PDF con `/P` sin modificar deja el elemento `aria-disabled` descrito por "Content
  editing is not permitted." y pulsarlo no hace nada.
- **Verificado en local (Windows):** `cargo fmt --all -- --check`; `cargo clippy --locked -p pdf-wasm
  -p pdf-save --all-targets -- -D warnings` nativo y `cargo clippy --locked --target
  wasm32-unknown-unknown -p pdf-wasm --all-targets -- -D warnings` (con `PDFIUM_WASM_DIR`), sin
  avisos; `cargo test --locked -p pdf-wasm -p pdf-save` (con `PDFIUM_DYNAMIC_LIB_PATH`) todo en
  verde (`pdf-wasm` lib 159, `pdf-save` lib 231, 0 fallos). `@vitela/core`: `build:wasm`,
  `typecheck`, Vitest 15 / 219, `build`. `@vitela/viewer`: `tsc -b`, Vitest 95 / 929, `build`.
  `apps/web`: `tsc -b`, Vitest 6 / 19, `vite build`. Playwright completo (Chromium, Firefox, WebKit,
  `CI=1`) **328 pasan, 1 falla, 22 omitidos**: el fallo fue `watermark.spec.ts:103` en WebKit
  (timeout, nada de contenido); aislado con `--repeat-each=5` pasa 5 / 5, y la pasada completa
  anterior (con el interruptor aún en las barras) no lo tuvo. `zero-network`, `phone-bar` y
  `embed` en verde.
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; compilan con clippy wasm32);
  `linux-gtk`, Android, `examples/embed-vite`.
- **No cubierto / pendiente:** ninguna edición (T-259+); Tab por los elementos de la página
  (opcional en la ficha, no hecho); `textRunLook`; sin lector de pantalla real.

### T-259 — resultado

Edición de runs de texto en el modo **Edit content**, más los arreglos de revisión de T-258b
(parte A). Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Edit content") y en
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API).

- **Parte A — seguimiento de la revisión de T-258b** (`contentModel.ts`, `useContentMode.ts`):
  (1) la clave de una lectura es `pageId/rotación/paintRevision/generación` (deshacer un giro
  conserva id y revisión); (2) con `editing.pending` no sale ninguna lectura (las lecturas van por
  posición y un paso de Organize o un deshacer puede poner otra página ahí) y una respuesta cuya
  posición tiene ya otra página se descarta y se vuelve a pedir; además, mientras corre un paso
  ninguna página se lee `ready` y una pulsación no elige nada (la lectura guardada puede nombrar
  lo que el paso va a cambiar: un clic justo tras **Undo** seleccionaba el run viejo y lo perdía al
  aterrizar — lo destapó el E2E); (3) `readsToSend`: como mucho 2 lecturas en vuelo, nunca una
  clave ya en vuelo, y lo que no sale se decide de nuevo al aterrizar una, contra las páginas
  deseadas entonces: una página que un scroll rápido ya pasó no se pide.
- **`pdf-wasm`** (`model/text_run.rs`, `engine/text_run.rs`, `dto/text_run.rs`):
  `DocumentModel::apply_text_run_edit(TextRunEdit)` con `Retype`, `Move { dx, dy }` (puntos PDF,
  y hacia arriba), `Remove` y `Restyle { runs, look }`; el run se resuelve contra la lectura
  pendiente (lo que `record_content_command` pliega) y el comando del core va por
  `DocumentModel::record_content` + las puertas de `pdf_save::content_edit`: un paso de deshacer
  cada uno; un segundo retipeo o movimiento del mismo run pliega en su paso; lo que no cambia nada
  (el mismo texto, mover 0, el aspecto que ya tiene) no registra nada; un `Restyle` de varios runs
  es un `Command::Batch`. El retipeo elige el comando como los shells nativos
  (`ReplaceTextRunWithInsertedFont` para fuente compuesta, `ReplaceTextRunContent` si no).
  Lecturas: `textRunLook` (`pdf_save::pending_text_run_look_of`: B/I/U/S, `canRemoveBold`,
  `canRemoveItalic`) y `textRunFontProgram` (`pdf_save::text_run_font_program_of`, que ya
  existía; `null` para standard-14 o un run solo pendiente). `applyEdit` con `retypeTextRun`,
  `moveTextRun`, `removeTextRun`, `restyleTextRuns`: refresco de vista previa y `paintRevision`
  como toda edición de contenido.
- **`@vitela/core`:** tipos `TextRunEdit`, `TextRunLook`, `TextRunLookState`, `FontProgram`;
  `Engine.textRunLook` / `textRunFontProgram`; el worker pasa `dx, dy` de la página dibujada
  (y hacia abajo) a puntos PDF.
- **`@vitela/viewer` (`features/content/`):** puro `textRuns.ts` (flechas 1 pt / Mayús 10 pt,
  umbral de arrastre 3 px, `retypeOf` — vacío = borrar —, `followRun`: el run editado se
  encuentra de nuevo por sitio y texto, no por id; familias CSS de reserva; `faceOf`;
  `editorBox` girado con la página), `runFont.ts` (el programa incrustado como `FontFace` si el
  navegador lo acepta, TrueType/OpenType; si no, la familia de reserva; cacheado por página, base
  y recurso), contenedor `useTextRunEditing` (sobre la cola `useAnnotationEdits.apply`: marca
  sucio y el guardado lo espera; cada paso comprueba al llegar su turno que la página en su
  posición sigue siendo la misma; nada se pide mientras corre un paso o se relee la página — las
  flechas de entretanto se suman en un movimiento —; el editor confirma con Enter y al perder el
  foco, cancela con Escape, y se cierra un turno después, nunca dentro de su propio blur, como en
  GTK; un guardado, salir del modo u otra selección confirman lo escrito), vistas `TextRunEditor`
  (campo camuflado: fuente, tamaño, color, peso e inclinación del run) y `TextRunToolbar` (Edit
  text, B/I/U con `aria-pressed`, Delete; no ofrece quitar una negrita que el core no puede
  quitar). `ContentLayer` informa doble clic y arrastre del contorno del run seleccionado.
  Enter/doble clic abren, Delete/Retroceso borran. Un run no retipeable lo dice.
- **Arreglos de esta pasada** (encontrados con el E2E, cada uno con su test en rojo primero):
  (a) lo de la parte A (2) sobre pulsaciones durante un paso; (b) las flechas pulsadas mientras
  se relee la página del run movido se perdían (la selección vieja ya no vale y `onKey` no las
  tomaba): ahora se suman y salen en cuanto el run se encuentra; (c) en Firefox una pulsación al
  lado del editor lo confirmaba y su blur lo confirmaba otra vez (Firefox mueve el foco aunque se
  cancele `pointerdown`): un editor abierto se confirma una sola vez.
- **Hallazgo del core, sin tocar:** al borrar (o editar) un run, el guardado normal deja en el
  fichero el flujo de contenido original **sin referenciar** (`pdf_edit::own_page_stream` copia
  el flujo antes de editarlo y deja la limpieza a la poda de un guardado comprimido; está
  documentado así en `pdf-edit`). El texto borrado no se ve ni se lee al reabrir, pero sigue en
  los bytes. Es igual en todos los shells; el E2E de borrado comprueba la relectura, no los bytes.
  Candidato a ficha propia (podar lo inalcanzable en el guardado completo).
- **Tests:** Rust 16 nativos nuevos (`model/text_run/tests.rs`: un paso por edición, plegado
  del segundo retipeo y movimiento, nada para lo que no cambia, restyle de varios = un paso,
  negrita leída/encolada/guardada, deshacer y rehacer a través de un checkpoint, puertas y su
  orden, programa de fuente incrustada y standard-14) y 2 `wasm-bindgen-test`
  (`tests/text_runs.rs`). `@vitela/core` 7 Vitest nuevos (1 `client`, 1 `handler`, 4
  `documentEngine`, 1 pdfium real: `reportlab_quote_paragraph.pdf` retipeado, movido, en negrita,
  guardado y releído, borrado y deshecho paso a paso). `@vitela/viewer` 37 nuevos (3
  `contentModel`, 8 `useContentMode`, 12 `textRuns`, 14 `useTextRunEditing` con
  `TextRunToolbar`/`TextRunEditor`).
- **E2E (`e2e/edit-text-runs.spec.ts`, 6 × 3):** retipeo en el sitio (editor sobre el contorno,
  en su tamaño), Enter, deshacer/rehacer, guardar y reabrir lee el texto nuevo; doble clic abre,
  Escape cancela, una pulsación al lado confirma; tres Mayús+→ = 30 pt en un paso y un arrastre
  más, guardado donde quedó; Delete borra, deshacer/rehacer, y el fichero reabierto no tiene run
  ahí; **B** en negrita, deshecho y rehecho, y releído en negrita tras guardar; un PDF que prohíbe
  modificar no ofrece ninguna edición.
- **Verificado en local (Windows):** `cargo fmt --all -- --check` limpio; `cargo clippy --locked
  -p pdf-wasm --all-targets -- -D warnings` nativo y `cargo clippy --locked --target
  wasm32-unknown-unknown -p pdf-wasm --all-targets -- -D warnings` (con `PDFIUM_WASM_DIR`), sin
  avisos; `cargo test --locked -p pdf-wasm -p pdf-save` (con `PDFIUM_DYNAMIC_LIB_PATH`) todo en
  verde (`pdf-wasm` lib 175, `pdf-save` lib 231, 0 fallos). `@vitela/core`: `typecheck`, Vitest
  15 / 226, `build`. `@vitela/viewer`: `typecheck`, Vitest 97 / 965, `build`. `apps/web`:
  `typecheck`, Vitest 6 / 19, `build`. Playwright completo (Chromium, Firefox, WebKit, `CI=1`)
  **347 pasan, 22 omitidos, 0 fallos**, con `zero-network`, `phone-bar` y `embed` en verde.
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; compilan con clippy wasm32;
  la misma superficie la cubre el Vitest con pdfium real); `linux-gtk`, Android.
- **No cubierto / pendiente:** tachado (`strike`) sin botón (la ficha pedía B/I/U); editor de una
  sola línea (sin bloques ni párrafos: T-260+); mover varios runs a la vez; sin lector de
  pantalla real.
- **Arreglos de la revisión de T-259** (cada uno con su test en rojo primero, Vitest y E2E):
  (1) **borrador perdido:** el editor vive en una ranura de página virtualizada; un scroll que la
  desmontaba no garantizaba blur y al volver se mostraba el texto original. El borrador vive ahora
  en `useTextRunEditing` (`OpenEditor.draft`, campo controlado) y al desmontarse el campo con el
  editor abierto se confirma lo escrito, como al perder el foco, un turno después (`detach` →
  `setTimeout`; un montaje inmediato — el doble montaje de React en desarrollo — lo cancela con
  `attach`), nunca dentro del desmontaje del elemento con foco. (2) **fuentes que se filtraban:**
  `runFont.ts` ya no tiene mapa de módulo: `createRunFontRegistry(engine, handle)` es un registro
  por documento que crea y libera `useTextRunEditing` (`dispose` hace `document.fonts.delete` de
  cada cara — también en el embed, donde caen en la página anfitriona — y no añade lo que llega
  después). La cara se guarda por el **programa** (los bytes que decodifica el core, comparados
  enteros tras un hash FNV-1a), no por el recurso `F1` (otro en cada Form XObject) ni por
  `baseGeneration` (un checkpoint reescribe la base, no el programa); un retipeo que inserta fuente
  trae otro programa y otra cara. Se descartó una clave de objeto desde `pdf-wasm`: el número de
  objeto no es estable entre checkpoints ni entre las fuentes importadas, y no tocó Rust. Un fallo
  de lectura no se guarda; un programa que el navegador rechaza sí. (3) **la pulsación del usuario
  gana:** con el editor abierto, una pulsación en otro run confirma el editor y selecciona lo
  pulsado; la re-selección pendiente del run retipeado la pisaba. `select` ahora re-apunta el
  seguimiento al run pulsado (por sitio y texto, en su página: cualquier edición deja viejas las
  lecturas de todas las páginas) o lo suelta si no es un run. (4) el editor se dibuja con el
  aspecto pendiente (`OpenEditor.look`, el `textRunLook` del core con el restyle encolado), no con
  el del fichero. (5) **división:** el overlay de runs sale de `DocumentViewer.tsx` a
  `features/content/TextRunOverlay.tsx` (sin cambio de conducta); los tipos de contenido y runs
  (`ContentRun`…`FontProgram`, `TextRunEdit`) salen de `core/types.ts` a `core/content.ts`,
  reexportados por `types.ts` y por `index.ts` (misma API pública). El "confirmar una vez" tiene un
  solo dueño (`ended` del hook; el campo ya no lleva el suyo) y `typing` es estable.
  E2E nuevos en `edit-text-runs.spec.ts`: escribir y hacer scroll lejos (la ranura de la página 1
  se desmonta) confirma y se guarda; una pulsación en otro run con el editor abierto deja ese run
  seleccionado; el editor sobre un run en negrita pendiente se dibuja en 700. Los tres en rojo
  contra el código anterior (Chromium). Sin cambios en Rust (ni fmt/clippy/test de `pdf-wasm`).
  Verificado en local (Windows): `@vitela/core` `typecheck`, Vitest 15 / 226, `build`;
  `@vitela/viewer` `typecheck`, Vitest 98 / 978, `build`; `apps/web` `typecheck`, Vitest 6 / 19,
  `build`. Playwright (`CI=1`) `edit-text-runs` + `edit-content` + `embed` + `zero-network` en
  los 3 navegadores: 66 pasan. Suite completa (`--reporter=line`): 352 pasan, 22 omitidos, 1
  fallo intermitente de `draw.spec` (WebKit, un clic que agotó el tiempo; 20 / 20 en verde al
  repetirlo solo). Otro intermitente de `draw.spec` en Chromium (1 / 40): el aviso "no se puede
  mostrar en Helvetica" del cuadro de texto que sigue visible tras reescribir, una carrera del
  diálogo de dibujo, que este cambio no toca (0 / 40 sobre el código anterior; sin arreglar aquí).

### T-260 — resultado

Edición de imágenes en el modo **Edit content**, más tres seguimientos de la revisión de T-259
(parte A). Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Edit content") y en
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API).

- **Parte A — seguimiento de la revisión de T-259** (cada uno con su test en rojo primero):
  (1) **edición atascada:** un commit hecho porque la página del editor salió de la vista
  (`detach`) pedía seguir el run en una página fuera de la ventana de lectura, que nunca se
  vuelve a leer: `content.following` quedaba en `true`, `busy` también, y las flechas se
  tomaban como movimientos de ese run (acumulados para cuando se leyera, con el
  `preventDefault` que quitaba el scroll con teclado). Ahora `useContentMode` suelta el
  seguimiento de una página que no está en `wanted` (no se ve seleccionado nada en ella de todas
  formas). Vitest (`useContentMode`) y E2E (`edit-text-runs.spec.ts`: tras el scroll, las
  flechas hacen scroll, un run de la página 12 se borra sin volver, y el run retipeado sigue en
  su sitio), los dos en rojo contra el código anterior. (2) **`runFont.ts`** pedía el programa
  entero al worker en cada apertura del editor antes de mirar la caché: ahora la primera clave es
  barata (id de página + recurso de la fuente + `baseGeneration`, `RunFontAt`) y los bytes solo
  se piden si falla; la cara se sigue deduplicando por los bytes. Se guarda también "sin
  programa" (standard-14) por esa clave; un fallo de lectura no. Hueco conocido, documentado en
  el módulo: dos Form XObjects de una página que llaman `F1` a fuentes distintas comparten la
  clave barata (el camuflaje del segundo puede salir en la cara del primero; el texto es el del
  core igualmente). (3) **El aspecto llega tarde:** el editor copiaba el `textRunLook` al abrir;
  si la lectura aterrizaba después, el camuflaje se quedaba sin negrita/cursiva. Ahora un efecto
  lo completa cuando llega (el estilo, `textRunStyle`, ya se actualizaba). Vitest.
- **`pdf-wasm`** (`model/image.rs`, `engine/image.rs`): `DocumentModel::apply_image_edit(ImageEdit)`
  con `Move { dx, dy }`, `Resize { to }`, `Remove`, `Replace { bytes }` e `Insert { at, bytes }`
  (puntos PDF, y hacia arriba), la imagen resuelta contra la lectura pendiente y el comando del
  core (`MoveImage`, `ResizeImage`, `RemoveImage`, `ReplaceImageSource`, `InsertImage`) por
  `DocumentModel::record_content` + las puertas de `pdf_save::content_edit`. Lo que no cambia nada
  (mover 0, la caja que ya tiene) no registra nada; una caja sin área o una distancia no finita es
  `UnsupportedOperation`; bytes que no decodifican, `InvalidImage` antes de encolar. **Un
  redimensionado tras un movimiento de la misma imagen pliega en su paso** (regla del core: una
  edición de geometría por imagen). `Replace` pregunta antes `check_image_replacement`.
  `checkImageReplacement(doc, page, id)` = `pdf_save::check_image_replacement` (permiso antes que
  la imagen, como `pdf-ffi`); nuevo `From<pdf_edit::EditError>` para `EngineError`. `Insert`:
  tamaño del core (`pdf_annotate::stamp_placement`, lado mayor 144 pt, esquina superior izquierda
  en el punto, como Android y GTK) y nombre `/XObject` aleatorio `VitelaImage<16 hex>`
  (`pdf_save::RandomIdGenerator`) que no use ninguna imagen de la página ni ninguna inserción o
  borrado encolado — como el adaptador Android: el core rechaza un nombre ocupado en vez de
  reutilizarlo, y un nombre secuencial podría chocar con lo que una imagen borrada dejó en los
  recursos tras guardar. Una imagen insertada (id sintético) se puede mover antes de guardar: el
  core toma un checkpoint. `image` (solo PNG) como dev-dependency de los tests.
- **`@vitela/core`:** tipo `ImageEdit` (`moveImage`, `resizeImage` con `rect`, `removeImage`,
  `replaceImage`, `insertImage` con `at`; geometría en la página dibujada, cruzada por
  `PageFrame`: `deltaToPdf`, `rectToPdf`, `pointToPdf`) y `Engine.checkImageReplacement`.
- **`@vitela/viewer` (`features/content/`):** puro `images.ts` (8 asas; `resizedRect`: una
  esquina conserva la proporción salvo con Mayús, un lado cambia una dimensión, mínimo 4 pt, la
  esquina o lado opuesto fijo; `handleBox`; `followImage`: la imagen editada se encuentra de nuevo
  por su caja, la más cercana entre las que solapan), contenedor `useImageEditing` (sobre la cola
  `useAnnotationEdits.apply` como los runs; flechas 1 pt / Mayús 10 pt sumadas mientras corre un
  paso; Delete/Retroceso; pregunta `checkImageReplacement` en cuanto la imagen está seleccionada
  y Replace abre el selector solo si dijo que sí — si no, dice la frase del core —; el fichero se
  comprueba como un sello: PNG o JPEG, `MAX_STAMP_BYTES`, `readStampImage` ahora exportado;
  **Insert image** elige el fichero y arma la siguiente pulsación en una página — como Android —,
  Escape o pulsarlo otra vez desarma; el tamaño se pide antes al core, `stampPlacement`, para
  seleccionar la imagen donde aterriza), vista `ImageOverlay` (asas, caja de vista previa durante
  el arrastre de un asa, barra **Replace** / **Delete**). `useContentMode` gana `followImage` y
  `followingKind` (el seguimiento sabe si busca un run o una imagen; una pulsación sobre una
  imagen durante un seguimiento lo re-apunta a ella); `useTextRunEditing` solo acumula flechas
  cuando sigue un run. `ContentLayer` arrastra también una imagen seleccionada por su cuerpo.
  `ContentStatus` gana **Insert image** (`aria-pressed` mientras está armado); la fila de estado
  dice "Click the page where the image goes. Press Escape to cancel." mientras tanto.
- **Desviación de los nativos, consciente:** los shells nativos redimensionan desde 4 esquinas
  libres; la web pone 8 asas y conserva la proporción desde una esquina por defecto (Mayús da la
  conducta nativa), como pedía la ficha. Mover una imagen con teclas es solo de la web.
- **Tests:** Rust 15 nativos nuevos (`model/image/tests.rs`: mover, plegar el segundo
  movimiento, redimensionar, nada para lo que no cambia, caja sin área, borrar, reemplazar con
  el `/Width` nuevo en el fichero y el viejo tras deshacer, bytes que no decodifican, insertar al
  tamaño del core, mover una inserción antes de guardar, dos inserciones con dos nombres, ids y
  páginas desconocidos, documento que prohíbe modificar, reemplazar tras mover a través de un
  checkpoint) — vistos en rojo con el cuerpo de `apply_image_edit` vaciado (13 de 15 caen) — y 2
  `wasm-bindgen-test` (`tests/images.rs`, compilan con clippy wasm32). `@vitela/core` 5 Vitest
  nuevos (1 `client`, 1 `handler`, 2 `documentEngine`, 1 pdfium real: el escaneo movido,
  redimensionado, reemplazado, con una imagen insertada a 144 × 72, guardado y releído, borrado y
  deshecho paso a paso). `@vitela/viewer` 30 nuevos (7 `images`, 3+1 `useContentMode` /
  `ContentStatus`, 16 `useImageEditing` con `ImageOverlay`, 2 `runFont`, 1 `useTextRunEditing`).
  Los de `useImageEditing`/`ImageOverlay` se escribieron antes de que el código corriera, pero la
  primera pasada fue ya contra la implementación: su rojo se comprobó después rompiendo el hook y
  la vista (9 de 16 caen).
- **E2E (`e2e/edit-images.spec.ts`, 6 × 3)** sobre un PDF con una imagen XObject de 2 × 1
  (`pdf.ts` gana `images`): mover con flechas y arrastre, un paso, guardado y reabierto donde
  quedó; esquina en proporción y con Mayús libre, un paso con el movimiento, guardado y
  reabierto; Delete y el Delete de la barra, deshacer/rehacer, nada en el fichero reabierto;
  Replace pregunta al core y luego el selector, el fichero guardado tiene el `/Width` nuevo en la
  misma caja; Insert image arma, Escape desarma, se inserta al tamaño del core, se mueve antes de
  guardar, deshacer/rehacer, guardado y releído; un PDF que prohíbe modificar no ofrece nada.
  Y el E2E de la parte A en `edit-text-runs.spec.ts`.
- **Verificado en local (Windows):** `cargo fmt --all -- --check` limpio; `cargo clippy --locked
  -p pdf-wasm -p pdf-save --all-targets -- -D warnings` nativo y `cargo clippy --locked --target
  wasm32-unknown-unknown -p pdf-wasm --all-targets -- -D warnings` (con `PDFIUM_WASM_DIR`), sin
  avisos; `cargo test --locked -p pdf-wasm -p pdf-save` (con `PDFIUM_DYNAMIC_LIB_PATH`) todo en
  verde (`pdf-wasm` lib 190, `pdf-save` lib 231, 0 fallos). `@vitela/core`: `build:wasm`,
  `typecheck`, Vitest 15 / 231, `build`. `@vitela/viewer`: `typecheck`, Vitest 100 / 1008,
  `build`. `apps/web`: `typecheck`, Vitest 6 / 19, `build`. Playwright `edit-images` +
  `edit-text-runs` + `edit-content` en los 3 navegadores (`CI=1`): 57 pasan. Suite completa
  (`CI=1 --reporter=line`): **372 pasan, 22 omitidos, 2 fallos**: `draw.spec.ts:53` en Chromium
  (el aviso de Helvetica del cuadro de texto que sigue visible, la carrera ya anotada en T-259;
  repetido solo 10 veces: 8 / 10) y `export-images.spec.ts:59` en WebKit (timeout en el radio
  "Current page"; repetido solo: 10 / 10). Ninguno toca contenido; `zero-network`, `phone-bar` y
  `embed` en verde.
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver; compilan con clippy
  wasm32; la misma superficie la cubre el Vitest con pdfium real); `linux-gtk`, Android.
- **No cubierto / pendiente:** girar una imagen; redimensionar con teclado; recortar; editar
  imágenes en una página con `/Rotate` solo lo cubre el E2E de 90° añadido en el seguimiento
  (mover y redimensionar; una inserción se ancla por su esquina superior izquierda en espacio
  PDF, como `stampPlacement`, sin test en página girada); sin lector de pantalla real. El
  hallazgo de T-259 sigue: un borrado deja el flujo original sin referenciar en un guardado
  normal (por eso el E2E comprueba la relectura y el `/Width` presente, no la ausencia).
- **Seguimiento de la revisión de T-260** (cada uno con su test en rojo primero):
  (1) **Insert image** devuelve el teclado a las páginas al armar (como Replace), así Escape
  desarma sin enfocarlas a mano; el E2E ya no las enfoca y comprueba el foco (rojo sin el
  arreglo). (2) **`place` respeta `busy`:** la pulsación queda guardada y se coloca cuando no
  corre ningún paso (también uno que la misma pulsación empezó al confirmar un retipeo); la
  imagen sigue armada hasta entonces y Escape suelta las dos. (3) **Replace** recuerda la
  `contentKey` de la página al abrir el selector; si al terminar de leer el fichero corre un paso
  o la página se leyó de nuevo, se rechaza con el aviso de página cambiada (el id ya no vale).
  `ContentMode` expone `keyAt`. (4) **El seguimiento** solo se suelta por estar fuera de la
  ventana cuando la ventana se conoce (`visible !== null`): un relayout de zoom sin ventana ya
  no lo pierde. (5) **Límites de decodificación** (`pdf_edit::decode`, nuevo):
  `probe_image` lee solo la cabecera y `decode_image` decodifica con `image::Limits`; los dos
  rechazan antes de reservar más de `MAX_IMAGE_SIDE` 16384 px por lado o `MAX_IMAGE_PIXELS`
  32 Mpx (los techos de `pdf-render`), con `max_alloc` de respaldo. Lo usan todos los sitios
  de sellos e imágenes de contenido: `image_xobject`, `build_stamp_appearance`,
  `stamp_from_image_bytes` (decodificación entera) y `stamp_placement` (solo cabecera). En
  `pdf-wasm` la comprobación previa de Replace/Insert es ahora solo de cabecera: el fichero se
  decodifica entero una vez, en la validación del registro (`record_content_command`), que
  rechaza antes de encolar uno con muestras rotas (tests nuevos). `into_rgba8` en vez de
  `to_rgba8` ahorra una copia. Test con un PNG de 69 bytes que declara 11000 × 11000.
  (6) **Duplicación:** `contentEdits.ts` (`ContentQueue`, `useGuardedApply`, `useHeldMove`)
  lo comparten runs e imágenes; `sameRect` vive en `contentModel`; el seguimiento guarda un
  `sought` explícito (`run` con texto, `image`, `removed`) en vez de texto vacío/`null`/caja
  ficticia; `ContentEditing.tsx` (`useContentEditing`, `ContentOverlay`, `ContentToolbar`) saca
  del `DocumentViewer` (593 → 476 líneas) el modo, las capas y el selector de fichero, sin
  cambio de conducta. (7) **E2E en página girada 90°:** mover y redimensionar en lo dibujado,
  guardar y releer en su sitio.
  Verificado en local (Windows): `cargo fmt --all -- --check` limpio; clippy `-D warnings`
  nativo (`pdf-wasm`, `pdf-save`, `pdf-annotate`, `pdf-edit`) y wasm32 (`pdf-wasm`) sin avisos;
  `cargo test -p pdf-wasm -p pdf-save -p pdf-annotate -p pdf-edit -p pdf-ffi` 1501 pasan, 0
  fallos. `@vitela/core` Vitest 15 / 231, `@vitela/viewer` 100 / 1015, `apps/web` 6 / 19;
  typecheck y build de los tres. Playwright (`CI=1`) `edit-images` + `edit-text-runs` +
  `edit-content` + `stamp` + `embed` + `zero-network` en los 3 navegadores en verde; suite
  completa **377 pasan, 22 omitidos, 0 fallos**.

### T-261 — resultado

Insertar texto y reescribir bloques/párrafos en el modo **Edit content**. Detalle en
[`apps/web/README.md`](../apps/web/README.md) ("Edit content") y en
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API). Hecho sobre
`fix/web-images-review-v` (#673, sin fusionar al empezar).

- **`pdf-wasm`** (`model/text_block.rs`, `engine/text_block.rs`): `DocumentModel::apply_text_block_edit
  (TextBlockEdit)` con `Retype { page, runs, text }` e `Insert { page, at, text, size }`, por
  `DocumentModel::record_content` y las puertas de `pdf_save::content_edit` como T-259/T-260.
  `Retype` busca el bloque en `read_paragraph_blocks` por sus `run_ids` exactos (si no es un bloque
  ahora, `UnsupportedOperation`: "léelo otra vez"), resuelve los runs contra la lectura pendiente y
  pide `pdf_save::retype_text_block_of`, como `pdf-ffi` `retype_text_block`: un párrafo envuelve cada
  línea tecleada a su ancho y pinta las que crecen como las suyas (`InsertTextRunLike`), un bloque de
  una línea pone todo en su primer run; `with_inserted_font` solo para un bloque de una línea con
  fuente compuesta (como Android). El mismo texto no registra nada; un texto vacío quita el bloque
  (`pdf_edit::remove_block`); un segundo retipeo del mismo párrafo pliega en su paso. `Insert` es
  `Command::InsertTextRun` en Helvetica estándar, esquina superior izquierda en el punto (como
  Android), tamaño 1–72 pt (los límites de Windows), una línea con algo que mostrar; el nombre
  `/Font` es aleatorio `VitelaText<16 hex>`, ni declarado por la página ni de una inserción
  encolada (el core *reutiliza* una fuente con ese nombre, `ensure_font_resource`, como anotan
  Android y Windows); el ancho lo mide el core al leer lo pendiente. `EditCommand.retypeTextBlock`
  / `insertText`.
- **`@vitela/core`:** tipo `TextBlockEdit` (`retypeTextBlock` con `runIds` y `text`, `insertText`
  con `at` en la página dibujada y `sizePt`), cruzado por `PageFrame.pointToPdf`.
- **`@vitela/viewer` (`features/content/`):** puro `textBlocks.ts` (`paragraphOf`: el párrafo — más
  de una línea — de un run; `paragraphKey`: Ctrl/Cmd+Enter confirma, Escape cancela, Enter es salto
  de línea, como GTK; `paragraphRetypeOf`, `insertionOf`, `insertedRect` girado con la página;
  `INSERT_TEXT_SIZE_PT` 14, el de GTK y Windows). `useDraftEditor` (nuevo, compartido por los dos
  editores nuevos): el borrador vive en el hook, confirmar/cancelar cuentan una vez, cierre un turno
  después, `detach`/`attach` para el scroll que desmonta la página, salir del modo confirma — las
  reglas de `useTextRunEditing` (que conserva su copia; no se tocó). Contenedores
  `useParagraphEditing` (lee `readParagraphBlocks` de la página del run seleccionado una vez por
  `contentKey` y nunca mientras corre un paso; **Edit paragraph** en la barra del run y
  Shift+Enter; rechaza con la frase de T-259 un párrafo con un run no decodificado; confirma un
  `retypeTextBlock` con la comprobación de `pageKey` y sigue la primera línea) y `useTextInsertion`
  (**Insert text** en la fila de estado, `aria-pressed`, devuelve el teclado a las páginas; la
  siguiente pulsación abre una caja vacía ahí — una por armado, como Insert image —; Enter o salir
  de ella registran la línea, Escape o vacía no; la línea nueva queda seleccionada donde aterriza
  y se puede retipear, mover o borrar antes de guardar: el core toma un checkpoint). Vistas
  `TextBlockEditor` (textarea sobre el párrafo con el tamaño, color e interlineado del primer run,
  ajuste al ancho, crece al añadir líneas) y `TextInsertEditor` (caja con contorno colgada del
  clic, 14 pt Helvetica, girada con la página sobre esa esquina). `ContentEditing` los monta: un
  guardado y una pulsación al lado confirman los tres editores; un solo inserto armado a la vez;
  con el editor de párrafo abierto la barra del run se oculta.
- **Desviación consciente:** el editor de párrafo usa la familia que sugiere el nombre de la
  fuente, no el programa incrustado (un párrafo puede mezclar fuentes); el tamaño del texto
  insertado es fijo (14 pt) como en GTK y Windows, sin el selector de Android.
- **Tests:** Rust 15 nativos nuevos (`model/text_block/tests.rs`: un paso por retipeo leído y
  guardado, línea que crece un interlineado abajo, plegado, bloque de una línea, nada para el mismo
  texto, vacío = quitar, runs que no son bloque, inserción en Helvetica al tamaño pedido y su
  nombre en el fichero guardado, dos inserciones dos nombres, retipear una inserción sin guardar,
  rechazos de texto/salto/tamaño/punto, `EncodingGap`, página fuera, documento que prohíbe
  modificar, párrafo con fuente incrustada) — 13 de 15 en rojo con el cuerpo vaciado — y 2
  `wasm-bindgen-test` (`tests/text_blocks.rs`, compilan con clippy wasm32). `@vitela/core` 2 Vitest
  nuevos (1 `documentEngine`, 1 pdfium real: el párrafo de `reportlab_quote_paragraph.pdf`
  retipeado y una línea insertada, guardados, releídos y deshechos). `@vitela/viewer` 34 nuevos (10
  `textBlocks`, 5 `useDraftEditor`, 9 `useTextInsertion` con `TextInsertEditor`, 10
  `useParagraphEditing` con `TextBlockEditor`), escritos antes que los módulos (rojo por módulo
  ausente).
- **E2E (`e2e/edit-text-blocks.spec.ts`, 5 × 3):** Insert text arma, Escape en las páginas
  desarma y en la caja no registra; la caja cuelga del clic a 14 pt, Enter registra, queda
  seleccionada, deshacer/rehacer, guardar y reabrir la lee; una línea sin guardar se retipea con el
  editor de runs y se guarda retipeada; un párrafo de dos líneas se abre desde la segunda, Enter
  añade línea y Ctrl+Enter confirma un paso que crece una tercera línea un interlineado abajo,
  deshacer/rehacer, y el fichero reabierto lee las tres como un párrafo; Shift+Enter abre, Escape
  cancela, una pulsación al lado confirma; un run de una línea no ofrece párrafo y un PDF que
  prohíbe modificar no ofrece insertar. 45 / 45 con `--repeat-each=3`.
- **Verificado en local (Windows):** `cargo fmt --all -- --check` limpio; `cargo clippy --locked
  -p pdf-wasm --all-targets -- -D warnings` nativo y wasm32 (con `PDFIUM_WASM_DIR`) sin avisos;
  `cargo test --locked -p pdf-wasm` (con `PDFIUM_DYNAMIC_LIB_PATH`) lib 209, 0 fallos.
  `@vitela/core`: `build:wasm`, `typecheck`, Vitest 15 / 233, `build`. `@vitela/viewer`:
  `typecheck`, Vitest 104 / 1051, `build`. `apps/web`: `typecheck`, Vitest 6 / 19, `build`.
  Playwright completo (`CI=1 --reporter=line`, 3 navegadores): **391 pasan, 22 omitidos, 1 fallo**
  intermitente: `keys.spec.ts:30` en WebKit (soltar un sello y deshacer tras pulsar la cabecera; no
  toca contenido; repetido solo 10 veces: 9 / 10, no comprobado contra la base).
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver); `linux-gtk`, Android.
- **No cubierto / pendiente:** insertar varias líneas de una vez (la caja es de una línea, como
  los nativos); elegir fuente o tamaño al insertar; el editor de párrafo sin el programa incrustado;
  un párrafo que crece puede tragarse el siguiente al releer (hueco conocido del core,
  #568); `useTextRunEditing` aún no usa `useDraftEditor` (candidato a
  unificar); editar párrafos en una página girada sin E2E propio.

### T-262 — resultado

Selección de varios elementos y portapapeles de contenido en el modo **Edit content**. Detalle
en [`apps/web/README.md`](../apps/web/README.md) ("Edit content") y en
[`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API). Hecho sobre
`feat/web-insert-text-v` (#674, sin fusionar al empezar).

- **`pdf-wasm`** (`model/selection.rs`, `model/clipboard.rs`, `engine/selection.rs`):
  `DocumentModel::apply_selection_edit(SelectionEdit)` con `Move { page, members, dx, dy }` y
  `Remove { page, members }` (`ContentMembers`: ids de runs, imágenes y trazados de la última
  lectura), resueltos contra la lectura pendiente y construidos por `pdf_edit::move_selection` /
  `remove_selection`, registrados por `DocumentModel::record_content` (un paso; un segundo
  movimiento de la misma selección pliega en el suyo; un rechazo toma checkpoint), como
  `pdf-ffi` `move_content_selection` / `remove_content_selection`. Una selección que es
  exactamente lo que añadió un pegado pendiente mueve el pegado mismo
  (`pdf_save::move_pending_paste`, el mismo paso); parte de uno se rechaza
  (`pdf_save::part_of_paste`); borrar uno entero va por checkpoint, o
  `paste_not_deletable` donde no se puede tomar. Mover 0 no registra nada.
  `copy_content_selection(page, members, for_cut)` = `pdf_save::checkpoint::copy_selection`
  (lo editado sin guardar se copia como se lee ahora, de la cola escrita en memoria, sin
  tocar la sesión); la puerta de la copia es la extracción de texto (`TextExtractionDenied`)
  y la de un corte `pdf_save::content_cut_allowed` antes de leer nada
  (`ContentEditRefusal::Cut` → `EditNotPermitted`, "cutting content is not permitted"),
  como `cut_content_selection`. `paste_content(page, clip, at)` = `pdf_save::record_paste`
  (esquina superior izquierda de la caja del clip en el punto, un paso), con las puertas de
  `pdf-ffi` `paste_content` (punto finito, `ensure_content_editing`, página). El clip cruza
  como `ContentClipEntry` (`takePdf`, `bbox`, `fromEncrypted`) y vuelve igual a
  `EditCommand.pasteContent`: el core no guarda estado de portapapeles. `record_content` se
  partió en `track_content_step`, que mantiene las marcas de la vista previa también para el
  pegado y el movimiento del pegado (que no pasan por la cola de `record_content`). Cada
  elemento leído dice ahora `pending` (`pdf_edit::pending_log_index`: solo lo sostiene una
  edición sin guardar), el equivalente de `ContentSelection.OnlyPending` de Windows.
  `EditCommand.moveContentSelection` / `removeContentSelection` / `pasteContent` y
  `Engine.copyContentSelection`. Fixture `pdfs::content_document_pages(n)`.
- **`@vitela/core`:** `ContentMembers`, `ContentClip` (`pdf` transferido, `bbox` del core,
  `rect` en la página dibujada, `fromEncrypted`), `SelectionEdit` (`moveContentSelection`,
  `removeContentSelection`, `pasteContent` con `at` en la página dibujada, cruzados por
  `PageFrame`), `Engine.copyContentSelection(handle, page, members, forCut?)` y `pending` en
  `ContentRun` / `ContentImage` / `ContentPath`.
- **`@vitela/viewer` (`features/content/`):** puro `selection.ts` (`membersOf`, `toggled`,
  `marqueeRect`, `gathered` — lo que la caja toca, como `ContentSelection.Gather` de Windows —,
  `boundsOf`, `soughtOf`/`followGroup` — cada elemento movido se encuentra otra vez por clase y
  sitio, nunca dos veces el mismo —, `pastedRect` girado con la página sobre su esquina,
  `pastedItems` — lo `pending` donde aterrizó —, `pastePoint` — regla de Windows: el puntero
  sobre una página, si no el sitio de origen en la primera página a la vista, dentro de la
  página —, `describeGroup`, `clipboardAction` — Ctrl/⌘ + C, X, V, nunca en un campo; copiar o
  cortar se dejan al navegador si hay texto suyo seleccionado). Contenedores `useContentGroup`
  (el grupo, válido mientras su página muestre la lectura en que se hizo; uno solo es la
  selección normal con su barra; tras un movimiento o un pegado se encuentra otra vez al
  releer — un pegado deja un grupo, o una imagen sola seleccionada como un clic, como GTK y
  Windows), `useGroupEditing` (arrastre, flechas 1/10 pt sumadas mientras corre un paso,
  Delete/Retroceso, Escape; un paso cada uno con la comprobación de `pageKey`) y
  `useContentClipboard` (Copy/Cut/Paste de la fila de estado y las teclas mientras el modo está
  activo y el foco en el editor, fuera de campos y diálogos; un corte es la copia para cortar y
  después la eliminación como un paso, y guarda el clip solo si la eliminación entró, como
  `cut_content_selection`; nada mientras corre un paso o se sigue un elemento editado).
  `clipStore.ts`: el clip vive en la sesión (`ContentClipContext`, que da el
  `DocumentWorkspace` por encima de sus visores — como `ViewerSession` en Android y
  `ViewerState` en GTK), así que se pega en otro documento abierto después. `ContentLayer`
  gana el marco (arrastre desde donde no hay nada; con Mayús/Ctrl/⌘ suma), Mayús/Ctrl/⌘+clic
  que añade o quita, el contorno de cada miembro dentro de una caja discontinua y el arrastre
  del grupo por cualquier punto de esa caja. `ContentStatus` gana el grupo **Clipboard**
  (Copy, Cut, Paste) y la fila se parte en líneas si no cabe. `useContentMode` expone
  `choose`, `itemAt` y `wants`; `ContentEditing` monta todo y su `clear` (Escape) suelta
  también el grupo.
- **Desviaciones conscientes:** el portapapeles no escribe en el del sistema (Windows pone el
  texto del clip ahí); Paste por botón usa la regla de Windows (sitio de origen) y no arma un
  toque como Android; los trazados solos siguen sin moverse (T-263), en un grupo sí.
- **Hueco del core encontrado (NO arreglado aquí):** `pdf_save::read_pending_page_content_of`
  devuelve vacío para una página insertada en la sesión y aún sin guardar
  (`PageBacking::Empty`): lo pegado (o insertado) en ella no se lee como pendiente, así que no
  se puede seleccionar hasta guardar; el guardado sí lo escribe (probado en `pdf-wasm`). Afecta
  a todos los shells por igual; cambiarlo es de `pdf-save` y merece su ficha.
- **Tests:** Rust 16 nativos nuevos (`model/selection/tests.rs` 6: mover texto, imagen y
  trazado como un paso leído y guardado, plegar el segundo movimiento, mover 0, borrar como un
  paso, vacío/id caducado/página fuera/distancia no finita, documento que prohíbe modificar;
  `model/clipboard/tests.rs` 10: copiar sin paso y con la caja de la unión, pegar en otra página
  como un paso leído `pending` y guardado, pegar en otro documento, mover todo lo pegado mueve
  el pegado — un paso —, parte de un pegado rechazada, borrar un pegado entero por checkpoint,
  copiar lo editado como se lee ahora, cortar necesita el permiso de modificar y copiar no — y
  el clip dice `from_encrypted` —, documento que niega el texto, pegado rechazado donde no se
  puede o fuera) — 12 de 19 en rojo con los cuerpos de mover/borrar/pegar vaciados — y 2
  `wasm-bindgen-test` (`tests/clipboard.rs`, compilan con clippy wasm32). `@vitela/core` 5
  Vitest nuevos (1 `client`, 1 `handler`, 2 `documentEngine`, 1 pdfium real: copiar la imagen
  del escaneo, pegarla en la página 2 de `vector_only.pdf`, empujarla — un paso —, guardar y
  releer, y cortar en el escaneo como un paso). `@vitela/viewer` 39 nuevos (14 `selection`, 6
  `useContentGroup`, 5 `useGroupEditing`, 10 `useContentClipboard` con `ContentStatus`, 4
  `ContentLayer`); `selection.test.ts` se escribió antes que el módulo; los de los hooks y la
  capa, después: su rojo se comprobó rompiendo cada uno (19 de 25 caen).
- **E2E (`e2e/edit-clipboard.spec.ts`, 4 × 3):** un marco desde el margen reúne una línea y una
  imagen, Ctrl/⌘+C copia (nada que deshacer), Ctrl/⌘+V con el puntero sobre la página 2 pega
  ahí y selecciona lo pegado, arrastrarlo sigue siendo un paso (deshacer/rehacer), y el fichero
  reabierto tiene la línea y la imagen en las dos páginas, lo pegado donde se arrastró; Cut
  quita una imagen de la página 1 como un paso y Paste la pone en el mismo sitio de la página 2
  a la vista (dos pasos, deshechos y rehechos), guardado y releído; Mayús/Ctrl+clic añade y
  quita, dos Mayús+Abajo mueven el grupo 20 pt como un paso, Delete borra un grupo marcado
  como un paso, guardado y releído; un clip copiado en un PDF se pega en el siguiente abierto,
  y Ctrl/⌘+V en la búsqueda no pega nada en la página.
- **Verificado en local (Windows):** `cargo fmt --all -- --check` limpio; `cargo clippy --locked
  -p pdf-wasm --all-targets -- -D warnings` nativo y wasm32 (con `PDFIUM_WASM_DIR`) sin avisos;
  `cargo test --locked -p pdf-wasm` (con `PDFIUM_DYNAMIC_LIB_PATH`) lib 225, 0 fallos.
  `@vitela/core`: `build:wasm`, `typecheck`, Vitest 15 / 238, `build`. `@vitela/viewer`:
  `typecheck`, Vitest 109 / 1090, `build`. `apps/web`: `typecheck`, Vitest 6 / 19, `build`.
  Playwright `edit-clipboard` en los 3 navegadores (`CI=1`): 12 / 12. Suite completa (`CI=1 --reporter=line`, 3 navegadores): **404 pasan, 22 omitidos, 0 fallos**.
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver); `linux-gtk`, Android.
- **No cubierto / pendiente:** el portapapeles del sistema; pegar un clip en una página girada
  sin E2E propio (la caja gira sobre su esquina, como una línea insertada); redimensionar un
  grupo; seleccionar entre páginas (la selección es de una página, como en los nativos); el
  hueco del core de arriba.

### T-263 — resultado

Mover, redimensionar y borrar un trazado (una regla, un borde de tabla, una caja rellena) en
el modo **Edit content**. Detalle en [`apps/web/README.md`](../apps/web/README.md) ("Shapes")
y en [`packages/vitela-core/README.md`](../packages/vitela-core/README.md) (API). Hecho sobre
`feat/web-content-clipboard-v` (#675, sin fusionar al empezar).

- **`pdf-wasm`** (`model/path.rs`, `engine/path.rs`): `DocumentModel::apply_path_edit(PathEdit)`
  con `Move { page, path, dx, dy }`, `Resize { page, path, to }` y `Remove { page, path }`, el
  trazado resuelto contra la lectura pendiente (`pdf_save::read_pending_page_content_of`) y
  grabado como `Command::{MovePath, ResizePath, RemovePath}` por
  `DocumentModel::record_content`, los mismos comandos que `pdf-ffi` `FfiEditCommand` de
  Android/Windows/Linux. Un paso cada uno; un segundo movimiento o redimensionado, o el
  borrado, del mismo trazado pliega en su paso (regla de `pdf_save::record`
  `with_queued_path`). Mover 0 o redimensionar a la caja que tiene no registra nada. Puertas
  en el orden de las imágenes: `ensure_content_editing`, extracción de texto, página, valores
  finitos y lados no negativos, id caducado, y un lado que el trazado tiene no puede quedar en
  nada (`UnsupportedOperation`); un lado plano (la altura de una regla) admite cualquier valor
  y el core lo deja plano (`pdf_edit::path::page_mapping`). El grosor de línea se conserva
  (el core reescribe coordenadas, no añade `cm`). `EditCommand.movePath` / `resizePath` /
  `removePath`.
- **`@vitela/core`:** `PathEdit` (`movePath`, `resizePath`, `removePath`, geometría en la
  página dibujada, cruzada por `PageFrame` como la de las imágenes), `CorePathEdits` y
  `pathCommand` en `worker/content.ts`.
- **`@vitela/viewer` (`features/content/`):** puro `paths.ts` (`pathHandles`: ocho en una
  forma, los dos de los extremos en una regla, ninguno en un punto; `resizedPathRect`: esquina
  libre — como los nativos —, Mayús mantiene la proporción, un lado plano queda plano y en su
  sitio, como `dragged_rect` de GTK; `followPath`). `contentModel.ts` gana `hits` — un
  contorno (trazado `stroke`) solo se toca a 3 pt de sus bordes, como `path_hit` de GTK, así
  que dentro de un marco está su celda — y `nearestTo` (lo que `followImage` hacía, ahora
  común). Contenedor `usePathEditing` (gemelo de `useImageEditing` sin la imagen: flechas 1/10
  pt sumadas mientras corre un paso, arrastre, asas, Delete/Retroceso; `pageKey` en su turno)
  y vista `PathOverlay` (asas + barra **Shape** con **Delete**). Las asas salen de
  `ImageOverlay` a `ResizeHandles.tsx` (`useHandleDrag` + `ResizeHandles`), compartidas, sin
  cambio de comportamiento. `useContentMode` sigue también un trazado (`followPath`; un clic
  en un trazado mientras aterriza otra edición lo sigue en vez de soltar el seguimiento).
  `ContentLayer` arrastra el trazado seleccionado (una regla de altura 0 dentro de 3 pt, un
  contorno por su borde). Las vistas sobre la página (`ContentOverlay`, `SelectedItemOverlay`)
  salen de `ContentEditing.tsx` a `ContentOverlay.tsx` (re-exportado), para que el
  contenedor no pase de 300 líneas.
- **Seguimiento de la revisión de T-262:** mientras se lee la copia de un corte (`clipping`),
  los contenedores de un elemento (`useTextRunEditing`, `useParagraphEditing`,
  `useImageEditing`, `usePathEditing`) lo cuentan como ocupado — sus barras y asas también, y
  el doble clic (`onOpen`) no abre el editor —, así que ninguna edición de un elemento se
  encola con ids que el corte va a borrar (el `clipboard` se crea ahora antes que ellos).
  `onKey` solo se traga Delete, Retroceso y las flechas mientras tanto (`heldWhileClipping`
  en `contentEdits.ts`); el resto de teclas sigue su camino.
- **Tests:** Rust 12 nativos nuevos (`model/path/tests.rs`: mover como un paso leído,
  guardado, deshecho y rehecho; segundo movimiento pliega; redimensionar y guardar; una regla
  se alarga y queda plana; mover y redimensionar pliegan en un paso; lo que no cambia no
  registra; caja que colapsa / negativa / no finita rechazada; borrar hasta deshacer; mover y
  borrar en un paso; id o página desconocidos; documento que prohíbe modificar; un trazado
  dentro de `q … Q` como los del E2E) escritos antes que `model/path.rs`, y 2
  `wasm-bindgen-test` (`tests/paths.rs`, compilan con clippy wasm32). `@vitela/core` 1 Vitest
  nuevo (`documentEngine`: los tres comandos con la geometría cruzada a PDF).
  `@vitela/viewer` 30 nuevos (9 `paths`, 11 `usePathEditing` con `PathOverlay`, 5
  `ContentEditing` — mover y borrar un trazado por teclado y arrastre, y el corte en curso:
  Delete/Retroceso/flechas tragadas sin editar, Enter y letras no, barras ocupadas, nada
  encolado hasta que entra la eliminación del corte —, 2 `ContentLayer`, 1 `useContentMode`, 1
  `useImageEditing` y 1 `useTextRunEditing` con `clipping`); `paths.test.ts` y el test de
  `ContentEditing` se escribieron antes que el código (el de `ContentEditing` cae con el
  `clipping ||` anterior).
- **E2E (`e2e/edit-paths.spec.ts`, 4 × 3):** una caja se mueve con dos Mayús+Abajo y un
  arrastre como un paso (deshacer/rehacer), guardada y releída donde quedó; una esquina la
  redimensiona libre y una regla (solo dos asas) se alarga por su extremo y queda plana, las
  dos guardadas y releídas; Delete borra la caja y la barra la regla, dos pasos deshechos y
  rehechos, y el fichero guardado no pinta ninguna; en una página girada 90° la caja se mueve
  y redimensiona como se ve y se guarda ahí. `e2e/pdf.ts` gana `PageSpec.paths` (caja `re f`
  o regla `m l S`, cada una en su `q … Q`).
- **Verificado en local (Windows):** `cargo fmt -p pdf-wasm -- --check` limpio; `cargo clippy
  -p pdf-wasm --all-targets -- -D warnings` nativo y wasm32 (con `PDFIUM_WASM_DIR`) sin
  avisos; `cargo test -p pdf-wasm` (con `PDFIUM_DYNAMIC_LIB_PATH`) lib 237, 0 fallos.
  `@vitela/core`: `build:wasm`, `typecheck`, Vitest 15 / 239, `build`. `@vitela/viewer`:
  `typecheck`, Vitest 112 / 1122, `build`. `apps/web`: `typecheck`, Vitest 6 / 19, `build`.
  Playwright `edit-paths` en los 3 navegadores (`CI=1`): 12 / 12. Suite completa (`CI=1
  --reporter=line`, 3 navegadores): **416 pasan, 22 omitidos, 0 fallos** (una primera pasada tuvo 1 fallo en WebKit, `keys.spec.ts` "undoes from the keyboard after a click on the site header": el PDF soltado no abrió en 30 s con la suite en paralelo; pasa 3 de 3 solo y la segunda pasada completa fue limpia).
- **No corrido en local:** los `wasm-bindgen-test` (sin chromedriver); `linux-gtk`, Android.
- **No cubierto / pendiente:** un trazado bajo un `cm` con giro (el core lo reescribe como
  cuatro lados, cubierto por los tests de `pdf-edit`, sin E2E propio); editar uno solo de los
  trazados que añadió un pegado aún sin guardar no tiene test propio en la web (lo decide el
  core).

### Fase 5 — tareas

| Tarea | Entrega | Hecho cuando |
|---|---|---|
| **T-264** ✓ | App instalable y offline tras la primera carga: manifiesto (`manifest.webmanifest`, iconos de la marca), service worker escrito a mano (`dist/sw.js`) que precachea el build entero (shell, worker del motor, los dos WASM) en una caché con versión por contenido, registro solo en producción y aviso "nueva versión, recargar" que nunca recarga con cambios sin guardar. | Vitest de la lista de precache, la versión, las reglas de petición, el manifiesto y el bloqueo de la actualización; Playwright sobre el build de producción: cargar, desconectar, recargar, abrir un fixture y pintarlo; manifiesto válido y servido. |
| **T-265** | Cabeceras de seguridad en `_headers`: `Content-Security-Policy` estricta (`default-src 'self'`, `script-src 'self' 'wasm-unsafe-eval'`, `worker-src 'self'`, `manifest-src 'self'`, `connect-src 'self'`, `object-src 'none'`, `frame-ancestors 'none'`) y la misma política en `vite preview` para que los E2E la sufran. | Playwright: la suite completa en los 3 navegadores sin violaciones de CSP (`securitypolicyviolation` = fallo); un test comprueba que la cabecera llega. |
| **T-266** | Abrir PDFs desde el sistema con la app instalada: `file_handlers` en el manifiesto + `launchQueue` (Chromium), con el mismo flujo que el selector. | Vitest del consumidor de `launchQueue`; Playwright (Chromium) simulando un lanzamiento con un `FileSystemFileHandle`; omitido con motivo en Firefox/WebKit. |
| **T-267** | Aviso del navegador al cerrar o recargar con cambios sin guardar (`beforeunload`), con el mismo `onDirtyChange` que usa la actualización. | Vitest del gancho; Playwright: con una edición pendiente, cerrar la página dispara el diálogo; guardado, no. |
| **T-268** | Smoke test tras el deploy: el job de `web-deploy.yml` pide a la URL publicada `/`, `/sw.js`, `/manifest.webmanifest`, una ruta de herramienta y un asset, y comprueba códigos, tipos y cabeceras (`no-cache` del worker, `immutable` de `/assets/*`). | El job falla con una cabecera o un tipo mal servido (probado contra `vite preview` en local). |

T-264 bloquea T-266 y T-267 (manifiesto y `onDirtyChange`). T-265 y T-268 son independientes.

### T-264 — resultado

La app es instalable y funciona offline tras la primera visita. Detalle en
[`apps/web/README.md`](../apps/web/README.md) ("Installable and offline (PWA)").

- **Manifiesto** (`apps/web/src/features/pwa/manifest.ts`, puro): `name`/`short_name`
  Vitela, `id`, `start_url` y `scope` `/` (la raíz de Cloudflare Pages; toda ruta de
  herramienta queda dentro), `display: standalone`, `theme_color` `#1b2a57` (`--accent`)
  y `background_color` `#ffffff` (`--surface`). Iconos reutilizados del repo, sin copias
  en el árbol: `assets/brand/vitela-app-mark.svg` (`any`), el icono macOS de 512 px
  (`any`) y el de Play Store de 512 px (`maskable`, la marca cabe en la zona segura).
- **Service worker escrito a mano, sin dependencias nuevas** (`apps/web/scripts/pwaPlugin.ts`,
  plugin de Vite solo en `build`): en `generateBundle` copia los iconos a `dist/icons/` y
  escribe `dist/manifest.webmanifest`; `transformIndexHtml` añade el `<link rel=manifest>`,
  `theme-color`, el icono y `apple-touch-icon`; en `writeBundle` (`order: 'post'`, ya con
  el worker del motor y los WASM en disco) lista `dist/`, calcula la lista de precache
  (`precache.ts`: `index.html` como `/`, todo lo demás salvo `sw.js`, `_headers`,
  `_redirects`, `404.html`, mapas y cualquier `.pdf`) y una versión de 16 hex del SHA-256
  de nombres + bytes, y empaqueta `src/features/pwa/sw.ts` (IIFE, `build` de Vite con
  `define`) en `dist/sw.js`. Se descartó vite-plugin-pwa/Workbox: el worker es una lista
  de precache sin rutas en tiempo de ejecución; una dependencia (y Workbox entero) no
  compensa.
- **Estrategia** (`swRules.ts`, puro): instalar precachea todo (~9 MB; el shell
  revalidado con `no-cache`, los assets con hash desde la caché HTTP si ya están);
  activar borra las cachés `vitela-*` de otros builds y reclama las páginas abiertas.
  Una navegación de mismo origen recibe el shell cacheado; un asset precacheado, su
  copia (cache-first); un fallo de caché va a la red y la respuesta vuelve a la caché.
  Otros orígenes, métodos que no son GET, URLs con query y todo lo que el build no
  escribió pasan a la red sin tocarse. La actualización llega como un `sw.js` nuevo con
  otra caché.
- **Actualización** (`updates.ts`, `UpdateBanner.tsx`): registro solo en producción
  (`import.meta.env.PROD`; ni el servidor de desarrollo ni Vitest lo registran) y un
  rechazo del navegador se ignora. Un build nuevo instalado mientras otro controla la
  página muestra "A new version of Vitela is ready." con **Reload**; nada recarga solo.
  Con cambios sin guardar el botón está desactivado ("Save your changes to reload into
  it.") y `apply` no hace nada; si entran ediciones entre el clic y el `controllerchange`,
  tampoco recarga; y un `controllerchange` que pidió otra pestaña no recarga esta. El
  estado sucio sale del visor: `DocumentWorkspace` gana `onDirtyChange` (el flag de
  "Unsaved changes." de `useSave`).
- **Privacidad:** la caché solo guarda ficheros del build. Un PDF se lee del disco y
  nunca se pide por HTTP, así que nunca pasa por el worker ni se cachea; la lista
  rechaza `.pdf` igualmente y el E2E comprueba que tras abrir uno no hay ninguno. La
  descarga del precache justo después de la primera carga trae ficheros de la propia
  app desde su propio origen, ningún dato del usuario. El test zero-network sigue
  corriendo con service workers bloqueados (`playwright.config.ts`).
- **Cabeceras y deploy:** `public/_headers` sirve `/sw.js` y `/manifest.webmanifest` con
  `Cache-Control: no-cache` (y el manifiesto con `application/manifest+json`). No hay
  CSP (ni en `_headers` ni en `index.html`), así que nada bloquea el worker ni el
  manifiesto; T-265 la añade con `worker-src` y `manifest-src 'self'`. El deploy sube
  `dist/` entero, así que `sw.js`, el manifiesto y los iconos se publican sin cambiar el
  job; sí se añadieron `assets/brand/**` y los dos PNG del icono a los `paths` de
  `web-deploy.yml` y `web.yml` y a `scripts/deploy-needed.mjs` (el logo de la cabecera
  ya venía de `assets/brand` y un cambio ahí no desplegaba). `tsconfig.node.json` gana
  `allowImportingTsExtensions` para importar el plugin con extensión (el cargador nativo
  de configuración de Vite avisaba sin ella).
- **Tests:** `apps/web` Vitest 38 nuevos (9 `precache`, 8 `swRules`, 3 `manifest`, 13
  `updates` con contenedor/registro/worker falsos, 2 `UpdateBanner`, 3 `App`), escritos
  antes que el código; `@vitela/viewer` 1 nuevo (`DocumentWorkspace`: `onDirtyChange`
  pasa a `true` con una edición y vuelve a `false` al abrir otro documento; cayó primero
  sin la prop); `deploy-needed.test.mjs` 1 nuevo (los iconos de la marca despliegan, el
  resto de esas carpetas no; cayó primero). E2E `e2e/pwa.spec.ts` (2 × 3): offline —
  cargar, esperar el precache (shell, worker y los dos WASM, todo del mismo origen),
  `setOffline(true)`, comprobar con un `fetch` no cacheado que de verdad no hay red,
  recargar, abrir `forms/reportlab_acroform.pdf` y pintarlo, abrir `/organize` y
  comprobar que la caché no ganó nada (ningún `.pdf`); y manifiesto — enlazado desde la
  página, JSON válido, iconos servidos con su tipo, `sw.js` servido como JavaScript.
  Pasa en los tres navegadores de Playwright en Windows, así que no hay omisiones. Se
  comprobó que la prueba offline no es vacía: con service workers bloqueados falla.
- **Verificado en local (Windows):** `@vitela/core` `build:wasm` y `build` (sin cambios).
  `@vitela/viewer`: `typecheck`, Vitest 112 / 1127, `build`. `apps/web`: `typecheck`,
  Vitest 11 / 57, `build` (`sw.js` precachea 13 ficheros); `node --test
  scripts/deploy-needed.test.mjs` 6 / 6. Playwright `pwa` en los 3 navegadores (`CI=1`):
  6 / 6. Suite completa (`CI=1 --reporter=line`, 3 navegadores, 444 tests): la última
  pasada **422 pasan, 22 omitidos, 0 fallos**. Tres pasadas anteriores tuvieron 1 o 2
  fallos, siempre en WebKit y cada vez en otro test (`phone-bar` export, `edit-images`
  Insert image, `edit-text-runs` scroll, `watermark` texto), todos timeouts de clic o de
  `requestAnimationFrame` con la suite en paralelo; cada uno pasa 3 de 3 solo, y en esos
  tests los service workers están bloqueados, como el flake de WebKit anotado en T-263.
- **No cubierto / pendiente:** el aviso de actualización solo tiene Vitest (un E2E
  necesitaría servir dos builds seguidos); la instalación en sí (el diálogo del
  navegador) no se puede automatizar; Lighthouse no se pasó. CSP, `file_handlers`,
  `beforeunload` y el smoke tras el deploy son T-265…T-268.

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
