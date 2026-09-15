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

4. ~~**Consecuencia a MEDIR, no a asumir:**~~ **MEDIDO en T-191 — confirmado.** Un
   documento que llegó con object streams y xref stream se reescribe hoy como objetos
   sueltos + xref clásico, y eso **sí infla**: +11,6 % sobre `vitela-sample`, +3,7 % sobre
   el fixture de ReportLab. La tabla completa está en T-191, y la afirmación quedó fijada
   por un test (`todays_full_rewrite_inflates_a_document_that_arrived_packed`) en vez de
   por esta línea. Así que sí: esta feature tapa una regresión de tamaño que ya existía en
   el full rewrite.
   **Y además apareció algo peor que inflar, que el hecho 4 no anticipaba:** el full
   rewrite de hoy *destruye* dos clases de documento. Un cifrado se carga como documento
   vacío y se reescribe como PDF sin páginas; un firmado colapsa sus revisiones y se lleva
   puesta la firma (−92 % de tamaño, que **es** la firma). Ver T-191.

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
- [x] T-191 (dep T-190) Pasada estructural: `use_object_streams` + `use_xref_streams` vía
      `save_with_options`, más `Document::compress()` sobre los streams sin filtrar.
      **Incluye medir el hecho 4**: tamaño de entrada vs. tamaño tras un full rewrite actual
      sobre todo el corpus, y dejar la tabla de resultados en esta ficha. [Compress]
      **(2026-09-15 — completo.** Módulo nuevo `core/pdf-compress/src/structural.rs`; el
      `pipeline.rs` deja de ser una costura vacía y pasa a ser sólo el orden de las etapas.
      Alta de `lopdf` y de `pdf-manip` en el `Cargo.toml` del crate.

      **Qué hace la pasada:** carga con `load_mem`, flatea todo stream que llegó **sin**
      `/Filter` (contando cuántos, que es la única razón por la que no se usa
      `Document::compress()` tal cual: devuelve `()` y el reporte tendría que adivinar), y
      escribe con `save_with_options` pidiendo object streams + xref stream.

      **La tabla del hecho 4** — reproducible con
      `cargo test -p pdf-compress --test corpus -- --nocapture measure`:

      | fixture | original | `save_to` de hoy | `compress()` | vs. original |
      |---|---:|---:|---:|---:|
      | `assets/sample/vitela-sample.pdf` | 2 060 | 2 060 (+0,0 %) | 1 382 | **−32,9 %** |
      | `content-edit/reportlab_embedded_subset.pdf` | 33 175 | 32 901 (−0,8 %) | 20 080 | **−39,5 %** |
      | `signed/rsa2048_sha256.pdf` | 34 169 | 33 754 (−1,2 %) | 34 169 | 0,0 % (rechazado) |
      | `signed/two_signatures_rsa2048_sha256.pdf` | 67 777 | 66 825 (−1,4 %) | 67 777 | 0,0 % (rechazado) |
      | `encrypted/rc4_128_user_and_owner.pdf` | 892 | **lo destruye** | 892 | 0,0 % (rechazado) |
      | `encrypted/aes_128_user_and_owner.pdf` | 1 010 | **lo destruye** | 1 010 | 0,0 % (rechazado) |
      | `large/edit_reopen_10pg.pdf` | 299 640 | 299 640 (+0,0 %) | 297 992 | −0,5 % |
      | `large/edit_reopen_50pg.pdf` | 13 680 638 | 13 680 638 (+0,0 %) | 13 671 752 | −0,1 % |
      | `large/perf_200pg.pdf` | 54 723 957 | 54 723 957 (+0,0 %) | 54 687 459 | −0,1 % |

      **El hecho 4, respondido de verdad.** Ninguno de esos fixtures llegó *con* object
      streams — todos se escribieron con xref clásico — así que la primera tabla no podía
      contestar la pregunta. La medición construye el caso que faltaba: empaqueta el
      fixture primero y **después** lo pasa por el `save_to` de hoy:

      | fixture (empaquetado primero) | empaquetado | `save_to` de hoy | delta |
      |---|---:|---:|---:|
      | `assets/sample/vitela-sample.pdf` | 1 774 | 1 980 | **+11,6 %** |
      | `content-edit/reportlab_embedded_subset.pdf` | 31 631 | 32 808 | **+3,7 %** |
      | `large/edit_reopen_10pg.pdf` | 297 992 | 299 145 | +0,4 % |
      | `large/edit_reopen_50pg.pdf` | 13 671 752 | 13 678 066 | +0,0 % |
      | `large/perf_200pg.pdf` | 54 687 459 | 54 713 585 | +0,0 % |

      Confirmado: el camino de guardado de hoy infla un documento que llegó empaquetado, y
      el efecto es proporcionalmente grande en archivos chicos (donde el xref clásico y los
      diccionarios sueltos son un porcentaje real del archivo) y despreciable en los
      raster-heavy (donde las imágenes son todo). Queda fijado por test, no por esta tabla.

      **Dos hallazgos que la medición encontró y el diseño no anticipaba.** Los dos son del
      mismo tipo: la garantía de "nunca agrandar" **no** los ve, porque los dos producen un
      archivo más chico.

      1. **Un documento cifrado se carga vacío.** `Document::load_mem` sobre un PDF cifrado
         *no falla*: devuelve un handle que no tiene más que el `/Encrypt` — el lector
         abandona apenas ve que no hay password, antes de desempaquetar un solo objeto (el
         mismo gotcha que `pdf_manip::open` ya documenta del lado de la carga).
         Re-serializar eso escribe un PDF válido, chiquito y **sin páginas**. Es más chico
         que la entrada, así que la garantía lo aceptaría y le entregaría al usuario un
         archivo sin su documento adentro. **La garantía protege contra crecer, no contra
         desaparecer.** La pasada ahora devuelve el cifrado intacto con
         `Refusal::EncryptedDocumentNotRewritable`.
      2. **Un documento firmado bajaba 92 % — y el 92 % era la firma.** Con la página
         perfectamente intacta, así que ni siquiera el chequeo de páginas lo agarraba. Un
         PDF firmado es una revisión base más un incremental update; cargarlo y
         re-serializarlo colapsa las dos en una y deja el `/ByteRange` de la firma
         describiendo bytes que ya no existen. Ahora se devuelve intacto con
         `Refusal::SignaturesWouldBeInvalidated`, que es **exactamente** lo que esa variante
         decía en T-190: *"el llamador no dijo que lo sabe; el archivo se deja en paz hasta
         que lo diga"*. T-195 es quien agrega el decirlo (decisión 7). Hasta entonces el
         default es el seguro, porque la alternativa es mostrarle a alguien "¡92 % más
         chico!" sobre un documento cuya firma se fue.
         La detección es `pdf_manip::document_has_signatures`, no una segunda opinión
         escrita acá — por eso entra `pdf-manip` como dependencia.

      **Dos guardas más, chicas:**
      - La pasada **relee** el candidato y cuenta páginas antes de ofrecerlo. Un repack que
         pierde páginas ganaría la comparación de tamaños; una parseada extra por compresión
         cuesta menos que esa clase de bug.
      - `SaveOptions::builder()` **no se usa**, y hay un test que lo fija: su
         `compression_level` arranca en `0` y `build()` lo pasa tal cual, así que un builder
         al que nadie le llamó `.compression_level()` escribe los object streams **sin
         comprimir** — lo contrario del objetivo. `ObjectStreamConfig::default()` es nivel 6.

      Verificado en Windows: ciclo TDD real — la primera corrida dio **4 fallas / 34
      pasadas**, y tres de esas fallas eran expectativas mías equivocadas sobre `lopdf`
      (`Stream::compress` sólo cambia el contenido si los bytes flateados ganan por más de
      los 19 que cuesta la entrada `/FlateDecode`, así que un content stream de una línea
      queda crudo y **bien**). Gates completos: `cargo fmt --check` (exit 0),
      `cargo clippy --workspace --all-targets -- -D warnings` (limpio) y
      `cargo test --workspace` → **1100 passed, 0 failed** (eran 1078 en T-190).**
- [x] T-192 (dep T-191) Poda: objetos huérfanos (no alcanzables desde el catálogo) y
      recursos duplicados por hash de contenido. Cero cambios visuales — verificado por
      render comparado, no por inspección del árbol. [Compress]
      **(2026-09-15 — completo.** Módulo nuevo `core/pdf-compress/src/prune.rs`, más dos
      movimientos de estructura que la poda forzó y que se explican abajo. Dependencia de
      desarrollo nueva: `pdf-render` (sólo tests).

      **El hallazgo que arrancó la tarea: el repack de T-191 no era idempotente.**
      Un documento que llegaba empaquetado volvía **más grande**, y crecía otra vez en cada
      vuelta. Medido sobre `assets/sample/vitela-sample.pdf`: 2 060 bytes de entrada,
      1 774 tras un repack, **2 023 tras dos**. La causa son dos fugas, no una:

      1. **Un objeto `/Type /XRef` muerto por vuelta.** `Document::save_internal` —el
         escritor clásico— filtra `[ObjStm, XRef, Linearized]` de los objetos que serializa
         (`lopdf-0.45/src/writer.rs:80`). `save_with_object_streams`, que es el que pedimos
         al activar `use_object_streams`, filtra **sólo `ObjStm`** (`writer.rs:149`). Un
         xref stream es `Object::Stream`, así que `ObjectStream::can_be_compressed` lo
         rechaza en su regla 1 y cae en `objects_to_write_directly`: se escribe entero, al
         lado del xref stream nuevo que el escritor agrega al final. No lo referencia nadie.
      2. **El espacio de ids no se devolvía nunca.** El escritor acuña el `ObjStm` y el
         `XRef` en `max_id + 1`, y `max_id` sólo sube. Al recargar y barrer esos dos objetos,
         `max_id` seguía diciendo que existían, así que la escritura siguiente acuñaba dos
         ids *por encima* y la tabla xref cargaba dos entradas libres más. Cada vuelta, dos
         más. Con el barrido solo, el crecimiento bajaba de +215 bytes a **+5**; los cinco
         los saca `reclaim_id_space`.

      **Por qué nadie lo había visto: la garantía lo estaba tapando.** El candidato inflado
      perdía la comparación de tamaños y el usuario recibía sus bytes originales con
      `NoGain`. Es decir, la red de seguridad funcionaba — y por eso el bug era invisible.
      El test `compressing_an_already_packed_document_changes_nothing_at_all` (T-190) pasaba
      **por el motivo equivocado**: no porque no hubiera nada que ganar, sino porque el
      repack había salido peor. Ahora pasa por el motivo correcto, y hay dos tests nuevos
      que lo fijan: `repacking_a_packed_document_produces_the_very_same_bytes` (punto fijo
      byte a byte) y `compressing_a_compressed_document_is_a_no_op_on_every_fixture` (lo
      mismo sobre el corpus real).

      **Qué hace la poda.** Dos barridas, en este orden porque la primera alimenta a la
      segunda: (1) **fusión de duplicados** — objetos con contenido idéntico colapsan en un
      sobreviviente y toda referencia a las copias se reapunta; (2) **barrido de
      inalcanzables** — se borra todo lo que el trailer no alcanza, incluidas las copias que
      el paso 1 acaba de dejar huérfanas. Un solo contador (`objects_dropped`) porque hay un
      solo camino de borrado. No hay caso especial para `/Type /XRef`: es inalcanzable, y
      cae por la misma razón que cualquier otra cosa.

      **Lo que la ganancia real resultó ser.** Sobre `edit_reopen_10pg.pdf` la poda tira 9
      objetos, sobre `perf_200pg.pdf` tira 199 — uno por página menos uno. Son los
      **content streams**: las diez páginas dibujan `q 612 0 0 792 0 0 cm /Im0 Do Q`, treinta
      bytes byte-idénticos, y cada una resuelve `/Im0` por su propio `/Resources`. La
      indirección por nombre es lo que hace seguro compartir el stream, y también la razón
      por la que los diccionarios de recursos **no** deben fusionarse con él. Fijado por
      `pages_that_draw_through_the_same_operators_share_one_content_stream`.

      | fixture | T-191 | T-192 | vs. original |
      |---|---:|---:|---:|
      | `assets/sample/vitela-sample.pdf` | 1 382 | 1 382 | −32,9 % |
      | `content-edit/reportlab_embedded_subset.pdf` | 20 080 | 20 080 | −39,5 % |
      | `large/edit_reopen_10pg.pdf` | 297 992 | **297 237** | −0,8 % (era −0,5 %) |
      | `large/edit_reopen_50pg.pdf` | 13 671 752 | **13 667 642** | −0,1 % |
      | `large/perf_200pg.pdf` | 54 687 459 | **54 670 813** | −0,1 % |

      **Lo que nunca se fusiona, y por qué.** Contenido idéntico no es lo mismo que
      identidad intercambiable. Quedan excluidos `/Type /Page`, `/Type /Pages`,
      `/Type /Annot`, `/Type /Catalog` y todo lo que el trailer referencia directo. La regla
      detrás de la lista: *un objeto que el resto de la aplicación edita por identidad no se
      fusiona nunca.* Dos páginas idénticas colapsadas dejan `/Kids [5 0 R, 5 0 R]` — el
      conteo de páginas sobrevive, así que ni la garantía ni la relectura de la sesión lo
      ven, y después una anotación puesta en la página dos aparece también en la uno.

      **La fusión corre hasta punto fijo.** Colapsar duplicados cambia a los objetos que los
      referenciaban, lo que puede volverlos idénticos a su vez: dos `FontFile2` iguales →
      dos `/FontDescriptor` iguales → dos `/Font` iguales. Una sola ronda juntaría los
      programas de fuente (los bytes grandes) y dejaría las dos capas de diccionarios
      arriba. Tope de 8 rondas, que no es una cota de corrección —parar antes sólo deja
      bytes en la mesa— sino un freno a un grafo patológico.

      **Un detalle de `lopdf` que hay que saber para no escribir la fusión mal:**
      `Stream` deriva `PartialEq` sobre **cuatro** campos, y uno es `start_position`, el
      offset del que se leyó el stream en el archivo original. Dos streams byte-idénticos
      del mismo documento nunca están en el mismo offset, así que `==` los declara distintos
      y la fusión no habría encontrado **nada**. `same_content` compara lo que se va a
      escribir: diccionario, contenido y `allows_compression`. Fijado por
      `lopdfs_own_equality_would_have_found_no_duplicates`.

      **Dos movimientos de estructura que la poda forzó** (regla de no-monolitos de
      `CLAUDE.md`, y cero cambio de comportamiento):
      - **`session.rs` nuevo**: la poda es una transformación *del grafo*, y `structural.rs`
        hacía load → trabajo → save adentro suyo. Meterla como otra etapa `bytes → bytes`
        costaba un parseo y una serialización completos por etapa (en `perf_200pg.pdf` son
        54 MB de ida y de vuelta) y obligaba a cada etapa a tener su propia copia del
        `SaveOptions`. Ahora hay un solo sobre: `session` abre, decide los rechazos
        (cifrado, firmado), presta el grafo, escribe y relee el conteo de páginas.
        `structural.rs` queda siendo sólo el flate sobre streams sin filtrar — object
        streams y xref stream son el *formato de escritura*, no una etapa.
      - **`test_fixtures.rs` nuevo**: `loose_document` la usan cinco módulos; vivía dentro de
        `structural::tests`, que ya no es su casa.

      **El criterio de aceptación, tomado literal.** `tests/render_unchanged.rs` rasteriza
      cada página de cada fixture comprimible dos veces —de los bytes de entrada y de los de
      salida— y compara píxel a píxel. No "parecido", no "dentro de un umbral": idénticos.
      Es la única verificación que no se puede engañar: una poda que se llevó un objeto de
      más produce un grafo que igual parsea, igual tiene el número correcto de páginas, y
      dibuja un cuadrado en blanco donde había una fuente. **Comprobado que el test tiene
      dientes**: mutando el barrido para que siguiera una sola referencia por objeto, el
      render acusó el golpe (`reportlab_embedded_subset.pdf` página 0, 4 989 de 1 938 816
      bytes distintos) — no se dejó pasar sola.

      **Lo que deliberadamente NO hace: renumerar.** Compactar agujeros en el medio del
      espacio de ids implica reescribir cada referencia del archivo y compra una entrada de
      xref por objeto podado. Es otra feature, con un radio de daño mucho mayor, y no se
      cuela acá.

      **Segundo commit: el corte por responsabilidad, y el bug que destapó.**
      `prune.rs` salió en 766 líneas, muy por encima de la señal de 350 de
      `scripts/check_maintainability.py` y de la regla de `CLAUDE.md`. Partido en
      `prune/mod.rs` (el orden), `prune/merge.rs` (las rondas y las exclusiones),
      `prune/sweep.rs` (alcanzabilidad e id space) y `prune/identity.rs` (*cuándo dos
      objetos son la misma cosa* — donde vive la trampa del `start_position`). Los tests de
      integración siguieron el mismo criterio: `tests/common/mod.rs` (el corpus),
      `tests/corpus.rs` (guardianes), `tests/measure.rs` (la tabla).

      **Y al partirlo apareció un bug que el archivo único tapaba: la fusión nunca
      convergía.** Como no borra nada, las copias colapsadas seguían en `objects`, seguían
      byte-idénticas a su sobreviviente, y cada ronda volvía a encontrarlas. El bucle corría
      las **8 rondas siempre**, redirigiendo el grafo entero ocho veces en cada documento.
      No se veía porque el contador que se usaba era el de la barrida, no el de la fusión;
      al mover los tests al módulo de la fusión, dos empezaron a reportar 8 y 21 en vez de
      1 y 3. Arreglado con un conjunto `retired` que excluye lo ya colapsado de las rondas
      siguientes — ahora termina en 2 rondas en el caso simple y en 4 en el encadenado.
      Esto es la regla de no-monolitos pagando sola: el corte no fue cosmético, encontró el
      bug.

      Tras el corte, **cero warnings de mantenibilidad en `pdf-compress`** (el total del
      repo bajó de 105 a 103).

      Verificado en Windows: ciclo TDD real — el primer `cargo test -p pdf-compress --lib`
      con los dos tests de idempotencia escritos y nada implementado dio **2 fallas / 40
      pasadas**. Gates completos: `cargo fmt --all -- --check` (exit 0),
      `cargo clippy --workspace --all-targets --locked -- -D warnings` (limpio),
      `cargo test --workspace --locked` → **1136 passed, 0 failed** (eran 1100 en T-191),
      `scripts/check_readme_tables.py` OK y `scripts/check_maintainability.py` sin ningún
      warning en el crate.**

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
      **Nota de T-191:** el default ya es el seguro — un documento firmado se devuelve
      intacto con `Refusal::SignaturesWouldBeInvalidated`. Lo que T-195 agrega no es el
      rechazo sino **el camino del sí**: cómo el llamador dice "sé que se invalida, dale".
      Mismo criterio para el cifrado: hoy se rechaza porque no se puede leer; T-195 es quien
      decide si, teniendo la password, se puede.
- [ ] T-196 (dep T-195) Exposición en `pdf-ffi` para los shells: preset, ejecución y
      lectura del reporte. [Compress, FFI]

### Fase 4 — Pruebas y fixtures
- [ ] T-197 (dep T-194) Corpus: un PDF de escaneo (imágenes grandes con pérdida), uno
      vectorial puro (donde la única ganancia posible es estructural), uno con transparencia
      real (`/SMask`), uno ya comprimido al máximo (caso `NoGain`), uno cifrado y uno
      firmado. Test guardián: **ningún fixture crece con ningún preset**. [CompressFixtures]
      **Nota de T-191:** el arnés ya existe — `core/pdf-compress/tests/corpus.rs` corre el
      guardián de "no crece", el de "no pierde páginas" y el de "protegido vuelve
      byte-idéntico" sobre el corpus que hay hoy. T-197 es **agregar filas a `CORPUS`**, no
      escribir el harness de cero. Falta el escaneo, el vectorial puro y el de
      transparencia real.
      **Nota de T-192:** el corpus vive ahora en `tests/common/mod.rs` y lo comparten los
      tres arneses — `corpus.rs` (los guardianes), `measure.rs` (la tabla del hecho 4) y
      `render_unchanged.rs` (píxel a píxel). **Agregar una fila a `CORPUS` alcanza**: los
      tres la levantan sola, incluido el render. Los protegidos se excluyen del render
      porque vuelven byte-idénticos y se detecta ahí, no por estar fuera de una lista.

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
