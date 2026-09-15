# Ficha de batch B24 — Compresión de PDF (reducir tamaño de archivo)

> Documentado 2026-09-15, a pedido explícito del usuario, para encolarlo — **no arranca
> ahora**. Mismo formato de ficha diferida que B22 (metadatos).
>
> Origen del scope: revisando el grid de Home del shell GTK4 quedó que **Compress es el
> único tile muerto que queda en Linux** — buscar `tool: None` sobre
> `apps/linux-gtk/src/app/home/tools.rs` devuelve una sola línea, la 90, y el rail ya no
> tiene ningún `set_sensitive(false)` desde que Protect cerró en T-188. Pero lo que falta
> **no es el shell: es el núcleo**. `README.md:103` lista "Compress PDF | Reduce file size"
> con la columna de crate en `—`, y no hay ningún crate en `core/` que reduzca tamaño.
> El shell son cinco líneas; esto es un batch entero. De ahí este documento.

**Dependencias:** B6 ✓ (crates core). Independiente de los shells, igual que B20/B21/B22.
Numeración: siguiente libre tras B23. Tareas: T-190 en adelante (T-189 es la última
usada, en B8).

## Hechos clave del formato y del proyecto

Esta sección existe para que la implementación no reinvente lo que ya está, y sobre todo
para que no confunda dos cosas que se llaman igual.

1. **`flate2::Compress` ya aparece en el repo y NO es esta feature.**
   `core/pdf-edit/src/parse/filter.rs` usa `Compress`/`Compression` para **re-encodear un
   content stream con FlateDecode después de editarlo** — es el códec del formato, la
   contraparte del decode. Reducir el tamaño de un archivo es otra cosa: resamplear
   imágenes, deduplicar recursos, empaquetar objetos. Una búsqueda de "compress" prende
   fuerte ahí y no significa que haya nada hecho.

2. **El guardado de hoy escribe con el camino más caro de lopdf.** `pdf-save` serializa
   siempre con `lopdf::Document::save_to` (`strategy.rs:451` en full rewrite,
   `strategy.rs:288` en incremental), y `save_to` delega en `save_internal`: **tabla xref
   clásica y cada objeto serializado por separado**.

3. **lopdf 0.45 ya trae las palancas estructurales y no las usamos.**
   `Document::save_with_options(target, SaveOptions { use_object_streams, use_xref_streams,
   linearize, object_stream_config })` y `Document::compress()` (que recorre los objetos y
   comprime todo `Object::Stream` con `allows_compression`). La compresión estructural no
   necesita dependencias nuevas: está en una dependencia que ya está en el árbol.

4. **Consecuencia a MEDIR, no a asumir:** un documento que llegó con object streams y xref
   stream se reescribe hoy como objetos sueltos + xref clásico. Ese camino **puede inflar**
   el archivo. La primera tarea de la fase 1 es medirlo sobre el corpus, no afirmarlo. Si se
   confirma, esta feature no solo agrega compresión: tapa una regresión de tamaño que ya
   existe en el full rewrite.

5. **Colisión de nombres, cuidado.** `pdf-save` ya tiene su propio `SaveOptions`
   (`strategy.rs:38`), que es el reloj y el generador de `/ID` inyectables para guardados
   byte-idénticos en CI. lopdf tiene otro `SaveOptions`, el de arriba. No se agrega un
   tercero: lo que entre nuevo se llama `CompressPreset`, y el puente a lopdf queda
   encapsulado dentro del crate nuevo.

6. **El decoder/encoder de imágenes ya está.** `image = "0.25"` con features `png` y `jpeg`
   es dependencia directa de `pdf-save` y de `pdf-edit`. Resamplear no agrega peso nuevo.

7. **Localizar las imágenes de una página ya está resuelto.** `pdf-edit` tiene el
   inventario completo: `parse/interpreter.rs:166` (`image()` → `LocatedImage`),
   `image_xobject_names` (`interpreter.rs:527`), y del lado de escritura
   `edit.rs::image_source_bytes` / `replace_image_source`. Un resampleador reusa eso; no
   escribe un segundo recorrido de XObjects.

8. **Las inline images están fuera de alcance físico hoy.** El lexer las trata como una
   operación opaca (`parse/lexer.rs:453`, `skip_inline_image`) y el intérprete no las
   reporta como items — está fijado por el test `an_inline_image_is_not_reported_as_an_item`.
   No es un olvido, es el contrato actual. Por eso quedan fuera de scope (abajo).

9. **Los documentos cifrados no reciben object streams.** Es decisión de lopdf, documentada
   en `writer.rs:28`: un documento cifrado se escribe con cada objeto serializado suelto.
   El preset lossless sobre un PDF protegido va a rendir menos, y eso hay que reportarlo,
   no esconderlo.

## Decisiones de diseño

1. **Crate nuevo: `core/pdf-compress`.** A diferencia de B22 (que se metió en
   `pdf-document`/`pdf-save` porque eran ocho pares clave/valor), acá hay política propia:
   tablas de calidad, reglas de resampleo, poda de objetos. `pdf-save` ya son trece módulos
   y su responsabilidad es orquestar el guardado, no decidir a cuántos DPI baja una foto.
   `pdf-compress` depende de `pdf-document`, `pdf-manip`, `pdf-edit` e `image`; `pdf-save`
   depende de `pdf-compress` y lo invoca.
   *Alternativa descartada:* meterlo en `pdf-save` como un módulo más. Se descarta porque
   invierte la dirección de la dependencia útil — el resampleador necesita el inventario de
   imágenes de `pdf-edit`, y `pdf-save` ya arrastra a todo el mundo.

2. **Comprimir NO es un `Command` y no es deshacible.** Es una transformación de archivo
   completo en tiempo de guardado, no una edición del modelo de páginas. Guardar la inversa
   de "re-encodé 400 imágenes" es guardar una segunda copia del archivo: absurdo. Mismo
   criterio que el strip de protección, que vive en el audit log y jamás en el EditLog.
   El `Document` y su `EditLog` quedan intactos; deshacer una compresión es no guardarla.

3. **Tres presets, no un slider.** Los números de calidad son una tabla fija en el núcleo,
   no valores sueltos que la UI inventa:
   - `Lossless` — solo estructura: object streams + xref stream, flate sobre streams sin
     filtrar, poda de objetos huérfanos y recursos duplicados. **No toca un solo píxel.**
   - `Balanced` — `Lossless` + resampleo de imágenes a 150 DPI efectivos + re-encode JPEG
     q≈75 de las que ya eran con pérdida.
   - `Small` — `Lossless` + 96 DPI + q≈55.

4. **Garantía dura: nunca agrandar.** Si la salida no es más chica que la entrada, la
   operación devuelve los bytes originales y reporta `NoGain`. Esta es la única regla que
   hace la feature confiable, y de paso cubre el riesgo del hecho 4. Se aplica en dos
   niveles: por imagen (si el re-encode sale más grande, se conserva la original) y por
   archivo.

5. **Nunca upscalear y nunca cambiar de familia de códec a ciegas.** Una imagen ya por
   debajo del DPI objetivo se deja byte-idéntica. Un PNG/Flate no se convierte a JPEG:
   JPEG no tiene canal alfa, y el `/SMask` se perdería. La preservación del `/SMask` va
   fijada por test, no por buena intención.

6. **Reporte, no solo bytes.** `CompressReport { before, after, images_resampled,
   images_skipped, streams_recompressed, objects_dropped, refusals }` — mismo canal de
   reporte que ya usa `graft_pages` en el batch de ensamblado. Lo que se rechaza se
   reporta; no se rechaza en silencio.

7. **Compuertas heredadas, no nuevas.** Documento cifrado → se consulta
   `rewrite::full_rewrite_blocker`, el mismo gate que ya decide si un PDF protegido puede
   reescribirse. Documento firmado → se avisa que la compresión invalida la firma, por el
   mismo camino que ya reporta `content.rs` para las ediciones de contenido. No se inventa
   una segunda política de seguridad.

## Límites de responsabilidad

- `pdf-compress` posee las tablas de calidad, el resampleo, la poda y la garantía de
  "nunca agrandar".
- `pdf-save` posee cuándo se invoca la compresión, las compuertas de cifrado y firma, y la
  escritura final.
- `pdf-edit` sigue poseyendo el inventario de imágenes de una página; `pdf-compress` lo
  consume, no lo duplica.
- El shell posee el preset elegido, el destino del archivo y mostrar antes/después.
- El núcleo no decide en qué carpeta se guarda, ni muestra tamaños formateados.

## Fases y tareas

### Fase 1 — Núcleo estructural (sin pérdida)
- [x] T-190 Crate `core/pdf-compress`: `CompressPreset`, `CompressReport`, `CompressError`,
      y la garantía de "nunca agrandar" implementada y testeada antes que cualquier
      optimización real (con una implementación que no hace nada, el test ya tiene que
      pasar). Alta en `Cargo.toml` del workspace. [Compress]
      **(2026-09-15 — completo.** `core/pdf-compress/` con cinco módulos por
      responsabilidad, según la regla de no-monolitos de `CLAUDE.md`: `preset.rs` (la tabla
      de calidad, única copia), `guarantee.rs` (la regla y `Compressed`), `report.rs`
      (`CompressReport`/`Outcome`/`Refusal`/`Work`), `pipeline.rs` (vacío a propósito, es
      donde aterrizan T-191/T-192/T-194) y `error.rs`. Alta del miembro en `Cargo.toml`
      del workspace, entre `pdf-edit` y `pdf-save`.
      **Cómo queda enforced la garantía:** `guarantee::compress_with` es el único
      constructor de `Compressed`, y `Compressed` es lo único que cruza el límite público
      con bytes adentro. Una etapa que produzca un archivo peor no puede publicarlo por
      olvido — no tiene dónde ponerlo. La comparación vive en una sola función (`choose`)
      porque la regla se necesita dos veces, por archivo y por imagen (T-194), y dos
      comparaciones separadas se desincronizan.
      **Tres decisiones que salieron al implementar, no estaban en la ficha:**
      (1) igual tamaño **pierde** — reescribir a los mismos bytes cuesta la firma del
      usuario y no ahorra nada; (2) un candidato descartado reporta `Work::default()`,
      porque al archivo que el usuario tiene en la mano no se le hizo nada y un reporte
      que dice "142 imágenes resampleadas" sobre bytes intactos es mentira de buena fe;
      (3) las refusals **sí** sobreviven al descarte: son hechos sobre el input y suelen
      ser la explicación de por qué no hubo ganancia. `Outcome` se deriva de `before`/
      `after` en vez de guardarse, así que no puede contradecir a los números.
      **Sin dependencias todavía, a propósito:** T-190 no abre un PDF. `lopdf` entra con
      T-191, `image` con T-194.
      Verificado en Windows: ciclo TDD real — primer `cargo test -p pdf-compress` con
      `compress_with` sin implementar dio **11 fallas / 12 pasadas**, y tras implementarla
      **23/23 verdes**. Gates completos: `cargo fmt --check` (exit 0),
      `cargo clippy --workspace --all-targets -- -D warnings` (limpio, incluido el crate
      nuevo) y `cargo test --workspace` → **1078 passed, 0 failed**. El gate de
      `linux-gtk` no se ejercita acá y no hace falta: `pdf-compress` no tiene dependencia
      de shell y en Windows ese crate compila vacío por `cfg(target_os)`.**
- [ ] T-191 (dep T-190) Pasada estructural: `use_object_streams` + `use_xref_streams` vía
      `save_with_options`, más `Document::compress()` sobre los streams sin filtrar.
      **Incluye medir el hecho 4**: tamaño de entrada vs. tamaño tras un full rewrite actual
      sobre todo el corpus, y dejar la tabla de resultados en esta ficha. [Compress]
- [ ] T-192 (dep T-191) Poda: objetos huérfanos (no alcanzables desde el catálogo) y
      recursos duplicados por hash de contenido. Cero cambios visuales — verificado por
      render comparado, no por inspección del árbol. [Compress]

### Fase 2 — Imágenes
- [ ] T-193 (dep T-190) Inventario de imágenes con DPI **efectivo** por colocación: los
      píxeles del XObject contra el tamaño al que la matriz lo dibuja. Una misma imagen
      colocada dos veces a escalas distintas manda la mayor. Reusa el localizador de
      `pdf-edit` (hecho 7). [Compress]
- [ ] T-194 (dep T-193) Resampleo y re-encode: nunca upscalear, nunca romper alfa,
      `/SMask` preservado, y "me quedo con la más chica" por imagen. Las imágenes que no
      se tocan quedan **byte-idénticas**, no re-escritas. [Compress]

### Fase 3 — Integración de guardado
- [ ] T-195 (dep T-191, T-194) Punto de entrada en `pdf-save`: compresión como paso previo
      a la escritura, con la compuerta de cifrado (`full_rewrite_blocker`) y el aviso de
      firma inválida. Sin `Command` nuevo y sin tocar el `EditLog` (decisión 2). [Compress]
- [ ] T-196 (dep T-195) Exposición en `pdf-ffi` para los shells: preset, ejecución y
      lectura del reporte. [Compress, FFI]

### Fase 4 — Pruebas y fixtures
- [ ] T-197 (dep T-194) Corpus: un PDF de escaneo (imágenes grandes con pérdida), uno
      vectorial puro (donde la única ganancia posible es estructural), uno con transparencia
      real (`/SMask`), uno ya comprimido al máximo (caso `NoGain`), uno cifrado y uno
      firmado. Test guardián: **ningún fixture crece con ningún preset**. [CompressFixtures]

### Fase 5 — Docs
- [x] T-198 README: la fila "Compress PDF" pasa de columna de crate `—` a
      `pdf-compress *(planned)*` (hecho en el mismo cambio que crea este documento, mismo
      criterio que T-175 en B22).

## Tareas de UI (agregar a la ficha de B8, docs/batches-b8-b13.md, cuando arranque)

- [ ] T-199 (dep B24) Habilitar el tile Compress en Home: sacar `tool: None` de
      `tools.rs:90`, darle `description` (hoy es `""`), rutear por `HomeTool::Compress`
      incluido el arranque en frío vía `pending_tool`. Diálogo con los tres presets y un
      estimado; ejecución en worker; al terminar, tamaño antes/después y chooser de destino.
      El icono ya existe: `assets/icons/compress.svg` y `Icon::Compress`. **Ojo:** este tile
      es hoy el único sujeto del test que fija el contrato "visible pero deshabilitado"
      (`tools.rs:394`); cuando se habilite hay que mover ese contrato a otro lado o
      justificar por qué deja de estar cubierto — no borrarlo y seguir. [CompressUI]

## Criterios de aceptación

- Un escaneo típico con `Balanced` pesa menos y sigue leyéndose sin artefactos visibles a
  100% de zoom.
- Un PDF ya optimizado devuelve `NoGain` y **los bytes originales**, no una copia
  equivalente-pero-distinta.
- `Lossless` sobre cualquier fixture produce un render píxel a píxel idéntico al original.
- Ningún preset, sobre ningún fixture, produce un archivo más grande que el de entrada.
- Una imagen por debajo del DPI objetivo sale byte-idéntica del pipeline.
- Un PNG con transparencia conserva su `/SMask` y su alfa tras `Balanced` y `Small`.
- Un documento cifrado que no puede reescribirse se rechaza por el gate existente, con el
  motivo en el reporte.
- Un documento firmado avisa que la firma queda inválida **antes** de escribir.
- Comprimir no crea ninguna entrada en el EditLog ni habilita el botón de deshacer.

## Fuera de scope (v1)

Subsetting de fuentes — es la ganancia grande que falta, pero necesita un crate de fuentes
nuevo y apoyarse en el trabajo Type0/CID que B21 dejó parcial; merece su propia ficha ·
Linearización / fast web view — es la fila vecina del README ("Optimize for web") y una
feature distinta: reordena el archivo para streaming, no lo achica; lopdf expone el flag
`linearize` en el mismo `SaveOptions`, así que la tentación de mezclarlas va a estar, y se
resiste · Inline images (hecho 8) · Re-encode JBIG2/CCITT de escaneos monocromos · OCR o
cualquier transformación que cambie el contenido · Borrar anotaciones, bookmarks o campos
de formulario "sin usar": eso es pérdida de datos disfrazada de compresión, y la decide el
usuario en Organize, no un preset.

## Orden de ejecución

Fase 1 → 2 → 3 → 4 lineal, TDD estricto (mismo criterio que B20/B21/B22). T-190 primero y
solo: la garantía de "nunca agrandar" tiene que existir y estar testeada antes de que haya
una sola optimización real, porque es la red bajo todo lo demás. La UI (T-199) arranca
recién con T-196 listo.
