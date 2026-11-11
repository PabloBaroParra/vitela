# Ficha de batch B26 — Shell web (Vitela en el navegador)

> Documentado 2026-11-11, a pedido explícito del usuario. **Fase 0 cerrada (GO, #555);
> fase 1 en curso: T-217 ✓ (#560), T-218 ✓ (#561), T-220 hecha.**
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
| **T-219** | `core/pdf-wasm` en el workspace (wasm-bindgen): abrir desde bytes (con/sin contraseña, dual), nº de páginas, dimensiones, render → RGBA, text runs, caracteres de página, búsqueda. | `wasm-bindgen-test` en navegador headless; job wasm32 en CI; cubierto por cargo-audit/deny del root. |
| **T-220** ✓ | Esqueleto `apps/web` (Vite + React + TS): worker que carga ambos WASM (con el alias), cliente tipado del worker, Vitest, Playwright con el test zero-network, `web.yml`. Matriz: Chromium, Firefox y WebKit (decidida 2026-11-11). | Build reproducible; Playwright abre un fixture y ve un canvas pintado; zero-network en verde. |
| **T-221** | Visor: abrir (selector + soltar archivo), diálogo de contraseña, scroll virtualizado, zoom (incl. rueda anclada al puntero). | Flujos Playwright: abrir normal, abrir cifrado, scroll a la última página, zoom. |
| **T-222** | Búsqueda y selección de texto (resaltado, copiar), respetando el permiso de extracción. | Flujos Playwright: buscar y saltar entre resultados; seleccionar y copiar; PDF sin permiso → bloqueado. |

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
