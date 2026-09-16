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
- [x] T-193 (dep T-190) Inventario de imágenes con DPI **efectivo** por colocación: los
      píxeles del XObject contra el tamaño al que la matriz lo dibuja. Una misma imagen
      colocada dos veces a escalas distintas manda la mayor. Reusa el localizador de
      `pdf-edit` (hecho 7). [Compress]
      **(2026-09-16 — completo.** Módulo nuevo `core/pdf-compress/src/images/` (`mod.rs`,
      el recorrido; `dpi.rs`, la aritmética y la regla de quién manda) y **una función
      pública nueva en `pdf-edit`**: `parse/placement.rs` →
      `page_image_placements(document, page_object) -> Vec<ImagePlacement>`. Alta de
      `pdf-edit` en el `Cargo.toml` de `pdf-compress`.

      **Por qué `pdf-edit` tuvo que exponer algo nuevo, si el hecho 7 decía que ya estaba.**
      Estaba el recorrido, no el dato. Lo que es público hoy es `read_page_content`, que
      entrega cada imagen con su `bbox` — y el `bbox` es la caja **alineada a ejes** del
      cuadrado unitario colocado. Para hit-testing (lo que hace un shell) está perfecto;
      para medir está mal: una imagen girada un cuarto de vuelta reporta su alto como
      ancho, y una con skew reporta una caja más grande que lo que se pintó. Quien mide
      píxeles contra papel resamplearía por el factor recíproco en cada foto rotada del
      documento. Lo que hace falta es la `ctm_at_paint`, que vivía en `LocatedImage` detrás
      de un `read_located_content` que es `pub(crate)`.
      Alternativa descartada: hacer público `read_located_content`. Entrega los spans de
      bytes de cada operación — el vocabulario de la *edición*— a un crate que no debe
      editar contenido. La función nueva contesta exactamente una pregunta y devuelve
      exactamente lo que hace falta para contestarla: el `ObjectId` del XObject, su nombre
      de recurso y la matriz. Fijado por
      `a_rotated_placement_reports_the_size_it_draws_not_the_box_it_covers`, que es el
      único test que cae si se vuelve a `ctm.a` en lugar de `hypot(a, b)` — comprobado
      mutando la implementación.

      **La regla que el módulo existe para sostener: manda la colocación más GRANDE.**
      La resolución no es propiedad de la imagen. Una foto de 1 000 px colocada a una
      pulgada son 1 000 DPI; el mismo objeto colocado a diez pulgadas son 100. Si un
      documento la coloca dos veces, las respuestas no se promedian ni se toma la primera:
      resamplear para satisfacer la chica deja la grande dibujando 150 px en diez pulgadas
      —quince DPI, destruida— mientras que resamplear para satisfacer la grande sólo deja
      la chica más nítida de lo necesario. La grande es la que se puede arruinar, así que
      es la que decide. **En DPI efectivo eso es el mínimo**, por eso `governed_by` es un
      `min` y no un `max`; y es por eje, porque una matriz puede estirar ancho y alto
      distinto. Comprobado que los tests muerden: cambiando `min` por `max` caen tres,
      incluido el de punta a punta.

      **La propiedad de seguridad, que no estaba en la ficha: el inventario se construye
      desde las COLOCACIONES, no desde `/Resources /XObject`.** Una imagen puede estar en
      el documento y ser invisible para este recorrido: escrita como **inline image**
      (hecho 8, que el lexer saltea entera). No se vio su matriz, así que no se puede medir
      su DPI. Un inventario leído del diccionario de recursos la listaría igual, sin
      colocación detrás — y la lectura natural de "sin colocación" es "nada la restringe",
      que es justo la lectura que resamplea una foto hasta la nada. Construido desde las
      colocaciones **no aparece**, y lo que no aparece no se toca. Fijado por
      `an_image_the_document_never_paints_is_not_in_the_inventory`, y el fixture lleva un
      `/Unplaced` permanente para que cada test del inventario diga algo también sobre la
      imagen que no debe incluir.

      **Corrección posterior (B21, descenso a los forms).** Cuando se escribió esto un
      form XObject también era invisible: el intérprete lo trataba como un `Do` opaco. Dejó
      de serlo, y `page_image_placements` quedó descendiendo al form pero resolviendo el
      nombre contra la página — que es peor que no verlo. Un form redefine `/Im0` (sus
      `/Resources` **reemplazan** las del llamador, no se fusionan), así que la colocación
      del form se le atribuía a la imagen de la página: y como `governed_by` es un `min`,
      un ícono de 64 px estirado a 200 puntos bajaba el DPI gobernante de la foto de 288 a
      23 y la foto se resampleaba al tamaño de un ícono con el que no tiene nada que ver.
      Ahora el nombre lo resuelve el intérprete en el scope que pintó
      (`LocatedImage::xobject`) y las imágenes dentro de un form entran al inventario como
      cualquier otra. Fijado por
      `a_form_that_shadows_an_image_name_does_not_govern_the_page_image`.
      El `/SMask` queda afuera por la misma razón y con la misma suerte: cuelga del
      diccionario de su imagen padre y nunca es operando de un `Do`. T-194 llega a él por
      el padre, que es la única forma en que se lo puede resamplear sin perder la
      alineación que lo hace máscara.

      **Qué reporta hoy.** La etapa corre sólo bajo un preset con `image_policy` —
      `Lossless` ni pregunta, y hay un guardián de corpus (`lossless_never_looks_at_an_image`)
      que falla si esa compuerta se afloja. Lo que cuenta es `images_skipped`: hoy **todas**
      las imágenes medidas, porque T-193 mide y T-194 resamplea. No es una cuenta de
      imágenes *presentes* sino de imágenes *medidas*, a propósito: una imagen que el crate
      no pudo ver colocada no es una imagen que decidió no resamplear.

      **Lo que la medición sobre el corpus real encontró, y que T-197 necesita saber:**

      | fixture | imágenes | píxeles | dibujada a | DPI efectivo |
      |---|---:|---:|---:|---:|
      | `large/edit_reopen_10pg.pdf` | 10 | 100 × 100 | 612 × 792 pt | 11,8 × 9,1 |
      | `large/edit_reopen_50pg.pdf` | 50 | 316 × 316 | 612 × 792 pt | 37,2 × 28,7 |
      | `large/perf_200pg.pdf` | 200 | 316 × 316 | 612 × 792 pt | 37,2 × 28,7 |

      Los otros seis fixtures no pintan ni una imagen. **Ninguna imagen del corpus de hoy
      supera siquiera los 96 DPI de `Small`** — es decir, T-194 no tendría nada que hacer
      sobre el corpus actual, y un test suyo que pase sobre estos archivos no probaría
      nada. Tampoco hay ningún fixture que coloque la misma imagen dos veces, que es el
      caso que la regla de arriba existe para resolver. Los dos huecos son de T-197.

      Verificado en Windows: `cargo fmt --all -- --check` (exit 0),
      `cargo clippy --workspace --all-targets --locked -- -D warnings` (limpio),
      `cargo test --workspace --locked` → **1159 passed, 0 failed** (eran 1136 en T-192),
      `scripts/check_readme_tables.py` OK y `scripts/check_maintainability.py` sin ningún
      warning nuevo (103, igual que antes; `pdf-compress` sigue en cero).**
- [x] T-194 (dep T-193) Resampleo y re-encode: nunca upscalear, nunca romper alfa,
      `/SMask` preservado, y "me quedo con la más chica" por imagen. Las imágenes que no
      se tocan quedan **byte-idénticas**, no re-escritas. [Compress]
      **(2026-09-16 — completo.** `core/pdf-compress/src/images/` pasa de dos módulos a
      cinco: `mod.rs` (el inventario) y `dpi.rs` (la aritmética) ya estaban; se suman
      `format.rs` (qué declara el diccionario), `codec.rs` (abrir el stream a muestras y
      volver a cerrarlo) y `rewrite.rs` (la decisión). `images::pass` ahora recibe la
      `ImagePolicy` y devuelve `images_resampled` de verdad. Alta de `image` 0.25 en el
      `Cargo.toml` del crate — mismas features que ya compila el resto del workspace, así
      que suma un dependiente, no una dependencia.

      **El techo, no el objetivo.** `resample::shrunk_to` devuelve `None` — no "redimensionar
      a sí misma" — cuando la imagen ya está en el objetivo o por debajo, y ese `None` es
      exactamente lo que `rewrite` convierte en **byte-idéntica**: el objeto no se reasigna.
      Es la diferencia entre invisible en un render y decisiva en un diff, y es lo que
      sostiene que "comprimir un archivo ya comprimido no cambia nada" siga siendo cierto
      en un documento lleno de fotos y no sólo en uno lleno de texto. Cada eje se decide
      solo: una matriz puede estirar ancho y alto distinto, y una imagen puede sobrar de
      detalle a lo ancho y estar justa a lo alto. Comprobado que muerde: sacando la
      compuerta `measured <= target` caen cinco tests, incluido el de punta a punta.

      **El error que cometí con el `/SMask`, y el test que lo corrigió.** Escribí primero
      que la máscara baja *por el mismo factor* que su padre. Es falso, y el test lo dijo
      antes que yo: una máscara de 300 muestras sobre una imagen de 600 dibujadas en la
      misma pulgada está a **la mitad de DPI** que su padre, o sea ya a mitad de camino;
      aplicarle el factor del padre la dejaría más gruesa que el color que oculta. Lo
      correcto es que hereda su resolución (`EffectiveDpi::shared_with`) y después se le
      aplica **el mismo objetivo**: las dos terminan en 150 DPI, que es para lo que existe
      tener un objetivo. La máscara sigue al padre y sólo si el padre efectivamente cambió;
      y no se cuenta aparte — una foto con canal alfa es *una* imagen resampleada, no dos.

      **La regla de "me quedo con la más chica" no es decorativa, y el caso que la hace
      real es estrecho.** Un resampleo que produce más bytes de los que reemplaza se tira.
      Pasa cuando la imagen venía guardada a una calidad JPEG baja y el preset la
      re-encodea a la suya, más alta: detalle que nadie pidió a un precio que nadie aceptó.
      Para que un test lo demuestre hace falta que el resampleo apenas achique — 160 DPI
      efectivos contra un objetivo de 150 recupera un 12% de las muestras, y re-encodear
      ruido a q75 cuesta muchísimo más que los q5 a los que estaba. Con una imagen a 600
      DPI el test *no* falla aunque se saque la compuerta, porque bajar a un dieciseisavo
      de los píxeles gana siempre. Medido, no razonado: sacando la comparación cae ese
      test y sólo ese.

      **Las familias nunca se cruzan.** Un flate no se convierte en JPEG para ganar bytes
      (decisión 5: JPEG no tiene alfa y el `/SMask` se iría con él), y un DCT vuelve a
      salir DCT. `codec.rs` tiene las dos direcciones en el mismo archivo a propósito: un
      cambio en una que la otra no acompañe es exactamente cómo una imagen sale de una
      compresión ilegible.

      **Lo que el crate se niega a tocar, y sigue byte-idéntico.** No es una lista de
      pendientes: es la política. Profundidad distinta de 8 bits (las muestras vienen
      empaquetadas con relleno de fila); todo espacio de color que no sea gris o RGB —
      `/Indexed` son *offsets* de tabla y el promedio de dos offsets es un tercer color sin
      relación con ninguno, `/Separation` y `/DeviceN` son tintas detrás de una transformada,
      `/DeviceCMYK` son cuatro canales que el decodificador devuelve como tres; cualquier
      cadena de filtros que no sea sin filtrar, `/FlateDecode` solo o `/DCTDecode` solo, y
      sin `/DecodeParms` (un predictor significa que los bytes bajo el filtro son
      diferencias entre filas, no muestras); y `/Decode`, `/Mask` o `/ImageMask`, que hacen
      que una muestra signifique otra cosa que su propio valor. Más un techo de 256 MiB por
      imagen decodificada, que es lo que impide que un stream que dice ser una estampilla
      se infle a un gigabyte en un crate que corre al guardar sobre lo que el usuario abrió.

      **Por qué el módulo quedó en cinco archivos y no en tres.** El primer corte fue
      `source.rs` (leer) + `resample.rs` + `rewrite.rs`, y `scripts/check_maintainability.py`
      marcó tres archivos sobre las 350 líneas. El CLAUDE.md de este repo dice que eso es la
      señal de partir, no de seguir agregando, así que se partió por responsabilidad real y
      no por cuota: `format.rs` contesta qué declara el diccionario (componentes y familia
      de almacenamiento) y `codec.rs` hace lo que depende de esa respuesta (decodificar y
      volver a codificar). `test_fixtures.rs` pasó a carpeta por el mismo motivo y con el
      mismo criterio: `mod.rs` son los *documentos*, `images.rs` son los *rásters*. El crate
      vuelve a cero warnings.

      **Sobre el corpus, sin novedad y a propósito.** T-193 ya había medido que ninguna
      imagen de los fixtures de hoy supera siquiera los 96 DPI de `Small`, así que
      `images_resampled` sigue en 0 sobre el corpus real — el guardián de `tests/corpus.rs`
      se mantiene, con el motivo corregido: ya no es "T-194 no existe" sino "acá no hay nada
      que hacer, y no debe inventarlo". El PDF de escaneo que sí lo ejercite es de T-197.

      Verificado en Windows: `cargo fmt --all -- --check` (exit 0),
      `cargo clippy --workspace --all-targets -- -D warnings` (limpio),
      `cargo test --workspace` → **1199 passed, 0 failed** (eran 1159 en T-193),
      `scripts/check_readme_tables.py` OK y `scripts/check_maintainability.py` en 103
      warnings, el mismo número que antes del cambio, con `pdf-compress` en cero.**

### Fase 3 — Integración de guardado
- [x] T-195 (dep T-191, T-194) Punto de entrada en `pdf-save`: compresión como paso previo
      a la escritura, con la compuerta de cifrado (`full_rewrite_blocker`) y el aviso de
      firma inválida. Sin `Command` nuevo y sin tocar el `EditLog` (decisión 2). [Compress]
      **(2026-09-16 — completo.** Módulo nuevo `core/pdf-save/src/compress.rs`
      (`save_document_compressed`, `compression_blocker`,
      `compressed_save_will_invalidate_signatures`, `CompressedSave`), más
      `core/pdf-compress/src/consent.rs` del otro lado de la frontera. Dependencia nueva:
      `pdf-save` → `pdf-compress`, en esa dirección y sólo en esa.

      **El camino del sí para la firma, que era el punto de la tarea.** `SignedDocuments`
      es el valor que el llamador pasa; `pdf_save::SignatureAcknowledgement` es el mismo
      valor del lado de acá, y `compress.rs` es el único lugar del repo donde las dos
      palabras están una al lado de la otra. Con `ProceedAndInvalidate` la compresión
      corre; sin él, `pdf-compress` devuelve el archivo intacto como venía haciendo.

      **Un hallazgo que forzó tocar el selector de escritor.** La compuerta de firma de
      `pdf-save` vive en la rama de reescritura, porque sólo una reescritura puede romper
      una firma. Pero *comprimir sin editar nada* —el caso exacto del botón "achicá este
      archivo"— tomaba el escritor incremental y esquivaba la compuerta por completo. Por
      eso existe `strategy::Compression`: una compresión pedida fuerza la reescritura, lo
      que hace que la compuerta que ya existía dispare para el caso que antes no veía.
      Sin eso, además, el append quedaba tirado a la basura por el repack que venía
      después. `compressed_save_will_invalidate_signatures` es la consulta que corresponde:
      sobre un archivo firmado sin ediciones, `will_invalidate_signatures` contesta `false`
      y ésta contesta `true`, y las dos tienen razón.

      **La decisión 7, corregida: para el cifrado no hay camino del sí, y tener la password
      no cambia nada.** La decisión decía consultar `full_rewrite_blocker` —el gate que
      responde si un PDF protegido puede reescribirse con su protección intacta— con la
      implicación de que un documento cuyas dos passwords tenemos podría comprimirse. No
      puede, y el motivo es anterior a cualquier política: `save_with_object_streams` de
      `lopdf` hace `return self.save_internal(target)` para un documento cifrado
      (`writer.rs:108`), porque los objetos ya están cifrados cuando llegan al escritor y
      la clave de archivo ya no existe. Se pide el repack y no se obtiene. Desde el otro
      lado llega la misma respuesta: `pdf-compress` tampoco puede *leer* un cifrado. Así
      que `compression_blocker` contesta la pregunta más angosta que el escritor sí deja
      abierta —*¿estos bytes van a salir cifrados?*— leyendo los mismos dos campos que lee
      `apply_encryption_for_full_rewrite`, para que no puedan estar en desacuerdo sobre lo
      que se está por escribir. `full_rewrite_blocker` se sigue consultando en ese mismo
      guardado, por `build_encryption_state`, en el camino del escritor; llamarlo de nuevo
      acá agregaría un motivo sin agregar una respuesta. **El camino del sí para un
      documento protegido es `SaveIntent::StripProtection`**, que es otra cosa para
      preguntarle a un usuario que "comprimí igual", y está bien que lo sea.

      **Por qué la compresión corre después del escritor y no adentro.** Object streams y
      xref stream se deciden al serializar. Pasarle el grafo a `pdf-compress` en medio del
      guardado y serializar acá tiraría el repack, porque el `save_to` final de
      `strategy.rs` es el escritor clásico. La compresión tiene que ser lo último que
      produce bytes. Cuesta una parseada y una serializada extra por guardado comprimido, y
      compra que el modo de falla de toda la feature sea "el guardado de siempre".

      **Dos archivos partidos, forzados por el propio cambio.** `session.rs` pasó a
      `session/mod.rs` (el sobre: el grafo, el tally, el formato de escritura, la relectura)
      + `session/gate.rs` (la puerta: los dos documentos que no entran, y sus dos razones
      que no son simétricas) — los tests nuevos lo habían empujado sobre las 350 líneas y
      la cabecera del módulo ya nombraba las dos responsabilidades por separado. Los tests
      de rechazo se mudaron de `pipeline.rs` a `gate.rs`, donde vive la decisión. El
      archivo de tests de integración se partió por la misma regla en `compressed_save.rs`
      (qué produce) + `compressed_save_gates.rs` (a quién se le niega), con
      `tests/compressed/mod.rs` compartido. `scripts/check_maintainability.py` vuelve a
      **103 warnings**, el mismo número que antes del cambio, con `pdf-compress` en cero.

      Verificado en Windows: ciclo TDD real — el test del camino del sí falló primero con
      *"consented compression produced 5082 bytes from 5082"* antes de que `open` mirara el
      consentimiento. Gates completos: `cargo fmt --all -- --check` (exit 0),
      `cargo clippy --workspace --all-targets -- -D warnings` (limpio),
      `cargo test --workspace` → **1217 passed, 0 failed** (eran 1199 en T-194),
      `scripts/check_readme_tables.py` OK.**
- [x] T-196 (dep T-195) Exposición en `pdf-ffi` para los shells: preset, ejecución y
      lectura del reporte. [Compress, FFI]
      **(2026-09-16 — completo.** Módulo nuevo `core/pdf-ffi/src/compress.rs` y
      dependencia nueva `pdf-ffi` → `pdf-compress`. Cinco funciones exportadas:
      `compress_presets`, `compression_refusal`,
      `compressed_save_will_invalidate_signatures`, `save_compressed_to_bytes` y
      `save_compressed_to_path`; seis tipos: `FfiCompressPreset`, `FfiCompressOutcome`,
      `FfiCompressRefusal`, `FfiCompressWork`, `FfiCompressReport` y `FfiCompressedSave`.

      **Las dos compuertas se preguntan en momentos distintos, así que son dos
      funciones.** `compression_refusal` contesta antes de ofrecer el botón —el
      documento protegido que no se puede comprimir de ninguna manera— y
      `compressed_save_will_invalidate_signatures` contesta la que sí tiene un sí. Esta
      segunda **no** es la misma pregunta que `will_invalidate_signatures`, que ya
      cruzaba: sobre un archivo firmado sin ediciones, la vieja contesta `false` y la
      nueva `true`. Un shell que pregunte la que ya existía avisa sobre otro guardado.

      **`save_compressed_to_path` devuelve el reporte donde `save_to_path` no devuelve
      nada.** No es asimetría por descuido: un guardado común no tiene resultado que
      contarle a nadie, y una compresión que el usuario pidió sí. Del mismo modo,
      `FfiCompressedSave` es **un** record y no dos valores de retorno, por la misma razón
      que `pdf_compress::Compressed`: no se pueden tomar los bytes sin ver qué se les hizo.

      **`Refusal` es `#[non_exhaustive]` y `CompressPreset` no lo es, y eso decide los dos
      `match`.** El de refusals lleva `Other { detail }` con el `Display` del core, para
      que una variante agregada río arriba llegue al shell como una frase mostrable y no
      como un rechazo que desaparece. El de presets es total y sin comodín a propósito:
      un cuarto preset es una decisión para reabrir acá (decisión 3), no algo para
      absorber en silencio. `Work` cruza como `u64` porque `usize` no tiene ancho en una
      FFI, y así toda conversión ensancha.

      **Un hallazgo que la UI de T-199 necesita saber, y que fue medido, no supuesto.**
      El primer test afirmaba `streams_recompressed > 0` sobre el documento de 8 páginas
      del generador y **falló con la compresión funcionando**. La sospecha inicial —que el
      escritor de `pdf-save` ya venía filtrando esos streams— es falsa: medido sobre los
      bytes del guardado, los 8 content streams llegan **sin** `/Filter`, de 51 bytes cada
      uno. Flate no le gana a 51 bytes, así que `pdf-compress` se niega correctamente a
      atribuirse un trabajo que no hizo. **La ganancia entera (2457 → 1511 bytes) es
      formato de escritura —object streams y xref stream—, y el formato no se cuenta en
      `Work`.** Conclusión operativa: *un `Work` en cero es compatible con un `Reduced`
      real*, y un diálogo que muestre "no se hizo nada" a partir del tally va a mentir
      sobre un archivo que acaba de perder un tercio de su tamaño. Lo que se lee es
      `outcome` y los bytes. Queda congelado en
      `a_real_saving_can_arrive_with_nothing_in_the_tally`.

      **Un refactor chico que el cambio forzó.** `DocumentState::save_input` reemplaza los
      tres literales de `pdf_save::SaveInput` que había en `document.rs`: las cuatro
      funciones con forma de guardado —común, comprimida, y las dos preguntas previas—
      tienen que describir **el mismo guardado**, o la respuesta que recibe un shell deja
      de ser sobre el archivo que va a escribir. Sin cambio de comportamiento; lo cubren
      los 43 tests de `smoke.rs` que ya existían.

      El registro del consentimiento de strip sigue donde estaba —en la frontera, no en
      `pdf-save`— y la compresión lo hace igual que el guardado común: comprimir con
      `StripProtection` deja la entrada en el `AuditLog`.

      Verificado en Windows: `cargo fmt --all -- --check` (exit 0),
      `cargo clippy --workspace --all-targets -- -D warnings` (limpio),
      `cargo test --workspace` → **1232 passed, 0 failed** (eran 1217 en T-195),
      `scripts/check_readme_tables.py` OK y `scripts/check_maintainability.py` en 103
      warnings, el mismo número que antes del cambio. `core/pdf-ffi/**` está excluido de
      ese script por `.maintainabilityignore` ("se revisa como contrato de integración"),
      así que las 450 líneas del módulo nuevo no disparan el aviso de tamaño: se justifican
      solas por ser una responsabilidad única —el vocabulario de compresión cruzando la
      frontera— y partirlas en tipos/funciones sería una capa artificial.**

### Fase 4 — Pruebas y fixtures
- [x] T-197 (dep T-194) Corpus: un PDF de escaneo (imágenes grandes con pérdida), uno
      vectorial puro (donde la única ganancia posible es estructural), uno con transparencia
      real (`/SMask`), uno ya comprimido al máximo (caso `NoGain`), uno cifrado y uno
      firmado. Test guardián: **ningún fixture crece con ningún preset**. [CompressFixtures]
      **(2026-09-16 — completo.** Cinco archivos nuevos **commiteados** en
      `tests/fixtures/compress/` (~510 KB en total), generados por
      `tests/fixtures/gen-fixtures/src/compress/` (`mod.rs` los documentos, `raster.rs` los
      píxeles). El cifrado y el firmado ya estaban: los cuatro de `encrypted/` y `signed/`
      son las dos filas que la compresión devuelve intactas. Arnés nuevo:
      `core/pdf-compress/tests/image_stage.rs` y
      `tests/fixtures/gen-fixtures/tests/compress_corpus.rs`.

      | fixture | para qué está | tamaño |
      |---|---|---|
      | `scan_200dpi.pdf` | una imagen con pérdida muy por encima de todo techo: 1700 × 2200 `/DCTDecode` sobre Letter | 238 KB |
      | `reused_image_two_scales.pdf` | *gobierna la colocación más grande* — un XObject pintado a 72pt y a 18pt (300 y 1200 DPI) | 88 KB |
      | `transparency_smask.pdf` | transparencia real: foto `/DCTDecode` con `/SMask` `/FlateDecode`, las dos a 300 DPI | 174 KB |
      | `vector_only.pdf` | una página de trazos y tipografía: la etapa de imágenes no debe encontrar nada | 8 KB |
      | `already_packed.pdf` | object streams, xref stream, todo flateado — sin holgura, así que `NoGain` | 3 KB |

      **Commiteados y no generados, que es la decisión de la tarea.** `tests/fixtures/large/`
      está en `.gitignore` por sus 50 MB, y `common::CORPUS` lee esas filas como opcionales:
      un checkout que no corrió el generador simplemente las saltea. Acá eso sería lo
      contrario de lo que se pide. **Ningún workflow de CI corre `gen-fixtures`** —
      verificado sobre los once workflows de `.github/workflows/`—, así que un fixture que
      no entra al repositorio es un guardián que nunca dispara en CI, que es exactamente el
      agujero que T-197 existe para cerrar. Por eso cada ráster es sintético y suave, y
      medio megabyte alcanza para las cinco filas.

      **Lo que la etapa de imágenes hace ahora sobre archivos reales, medido:**

      | fixture | Lossless | Balanced | Small |
      |---|---:|---:|---:|
      | `scan_200dpi.pdf` | −0,0 % | **−35,4 %** (1700→1275) | **−66,8 %** (1700→816) |
      | `reused_image_two_scales.pdf` | −0,1 % | **−77,2 %** (300→150) | **−90,3 %** (300→96) |
      | `transparency_smask.pdf` | −0,0 % | **−82,4 %** (600→300, máscara incluida) | **−91,4 %** |
      | `vector_only.pdf` | −67,1 % | −67,1 % | −67,1 % |
      | `already_packed.pdf` | 0,0 % `NoGain` | 0,0 % `NoGain` | 0,0 % `NoGain` |

      Antes de estas filas, `images_resampled` era **0 sobre todo el corpus** y ningún test
      de T-194 tocaba un solo píxel de un archivo real.

      **Las aserciones son sobre cantidad de muestras, no sobre bytes.** "Quedó más chico"
      lo cumplen muchísimas respuestas equivocadas; "esta imagen ahora tiene 1275 muestras
      de ancho, que son 150 DPI sobre 612 puntos de papel" lo cumple una sola. El caso que
      mejor lo muestra es el del XObject reutilizado: tomar el **máximo** de las colocaciones
      se lee perfectamente razonable —"satisfacé la colocación más exigente"— y produce una
      imagen de 37 × 37, con la miniatura nítida y la colocación grande destruida. Las dos
      respuestas achican el archivo; sólo la cantidad de muestras las distingue.

      **Dos intentos del ráster que pasaban y no probaban nada, porque esto es lo que un
      fixture commiteado tiene de traicionero.** Un degradado continuo se guarda carísimo
      (600 × 600 RGB = un cuarto de megabyte) porque cada fila difiere de la anterior en
      todos sus bytes. Cuantizarlo arregla el tamaño y **rompe el fixture**: el resampleo
      *promedia*, o sea vuelve a poner todos los valores intermedios que la cuantización
      sacó, y la imagen reducida termina pesando **más** que la original. Ahí `rewrite`
      aplica correctamente su regla de "más chica o nada", la descarta, y el corpus queda
      con un fixture de transparencia que reporta `images_skipped: 1` en los tres presets.
      Lo mismo con baldosas planas. Lo que funciona es degradado + ruido de amplitud baja:
      la única forma en que el peso guardado acompaña a la cantidad de muestras, que es la
      premisa de todo esto. Medido, no razonado — con las dos primeras versiones el corpus
      no resampleaba nada.

      **Una fila que el enunciado pedía y no tenía archivo: `NoGain` sin negativa.** El
      corpus ya llegaba a `Outcome::NoGain` por dos archivos cifrados y dos firmados, pero
      en los cuatro casos es una **negativa**: el crate no los reescribe. `already_packed.pdf`
      llega al mismo veredicto por el camino opuesto —se reescribe, se mide, y no hay nada
      que ganar— y por eso su guardián exige además que `refusals()` esté vacío. Son dos
      rutas distintas que terminan en la misma palabra, y hasta ahora sólo una tenía un
      archivo real detrás. Dato al pasar: el `save_to` de hoy **infla** ese archivo un 7,9 %,
      que es el hecho 4 otra vez, ahora sobre un archivo del repositorio.

      **Un criterio de aceptación que decía "Balanced y Small" y sólo tenía Balanced.**
      Repasando los criterios contra los tests apareció que los cuatro tests de máscara de
      `images/rewrite.rs` corren todos con la constante `BALANCED`, y el de corpus también:
      **nadie ejercitaba un `/SMask` bajo `Small`**. `a_soft_mask_follows_its_image_down_...`
      es ahora un loop sobre los dos presets — 600 muestras sobre 144 puntos son 300 DPI,
      así que da 300 y 192. Comprobado que el caso nuevo muerde y no sólo pasa: sacando la
      llamada a `soft_mask` en `rewrite::shrink`, la iteración de `Small` falla sola con
      *"Small left the mask at a resolution its image no longer has: left (600, 600),
      right (192, 192)"*.

      Lo que este fixture **no** puede distinguir, y conviene decirlo: como la máscara es
      del mismo tamaño que su padre, "mismo factor" y "mismo objetivo" dan el mismo número.
      La regla que T-194 corrigió sigue apoyada en el test unitario de máscara a media
      resolución, no acá.

      **El guardián de abajo del guardián.** `compress_corpus.rs` pregunta lo que está un
      escalón antes: ¿los archivos son los que los tests de al lado creen estar leyendo? Un
      fixture que dejara de traer `/SMask`, o de estar muestreado por encima del techo,
      dejaría pasando a todos los guardianes sin que prueben nada. Como este generador
      **sí** es byte-reproducible (nada en él es aleatorio, a diferencia del corpus firmado
      con sus claves frescas), el último test fija los archivos commiteados contra el
      generador: editar un builder sin regenerar falla ahí y dice qué comando correr.

      Un detalle encontrado escribiendo eso: en el archivo empaquetado, el único stream sin
      `/Filter` es el propio `/XRef`, que lopdf escribe sin comprimir. No es holgura del
      documento —se reconstruye entero en cada guardado—, así que la aserción se acotó a los
      streams de contenido en vez de aflojarla.

      **Cuatro archivos y no uno, por la misma razón que T-194.** El primer corte fue un
      `compress/mod.rs` solo, y `scripts/check_maintainability.py` lo marcó en 506 líneas.
      El CLAUDE.md dice que eso es la señal de partir, así que se partió por responsabilidad
      real: `mod.rs` es qué filas hay y cómo llegan al disco, `documents.rs` son las cinco
      formas (cuántas páginas, qué se pinta y a qué tamaño), `images.rs` es cómo un conjunto
      de muestras se vuelve un XObject de imagen, y `raster.rs` son las muestras. Es el
      mismo corte que el crate bajo prueba hace entre `format`/`codec` y el resto: "qué
      declara este diccionario" no es la misma pregunta que "qué pinta esta página".

      Verificado en Windows: `cargo fmt --all -- --check` (exit 0),
      `cargo clippy --workspace --all-targets -- -D warnings` (limpio),
      `cargo test --workspace` → **1251 passed, 0 failed** (eran 1232 en T-196),
      `scripts/check_readme_tables.py` OK y `scripts/check_maintainability.py` en 103
      warnings, el mismo número que antes del cambio: ningún archivo nuevo dispara el aviso
      de tamaño.**

### Fase 5 — Docs
- [x] T-198 README: la fila "Compress PDF" pasa de columna de crate `—` a
      `pdf-compress *(planned)*` (hecho en el mismo cambio que crea este documento, mismo
      criterio que T-175 en B22).

## Tareas de UI (listadas en la ficha de B8, docs/batches-b8-b13.md)

- [x] T-199 (dep B24) Habilitar el tile Compress en Home: sacar `tool: None` de
      `tools.rs`, darle `description` (era `""`), rutear por `HomeTool::Compress`
      incluido el arranque en frío vía `pending_tool`. Diálogo con los tres presets;
      ejecución en worker; al terminar, tamaño antes/después y chooser de destino.
      El icono ya existía: `assets/icons/compress.svg` y `Icon::Compress`. **Ojo:** este
      tile era el único sujeto del test que fija el contrato "visible pero deshabilitado";
      al habilitarse hubo que mover ese contrato a otro lado — no borrarlo y seguir.
      [CompressUI]
      **(2026-09-16 — completo, en `apps/linux-gtk/src/app/write/compress/`. La ficha larga
      está en [batches-b8-b13.md](batches-b8-b13.md), donde esta sección pedía que se
      listara. Dos cosas que este documento pidió y que la implementación cambió a
      propósito, y por qué:**

      **(1) "un estimado" no existe, y no debería.** El diálogo iba a mostrar un ahorro
      previsto por preset. Nada en `pdf-compress` puede contestarlo sin correr —T-193 midió
      un corpus donde ninguna imagen supera siquiera los 96 DPI de `Small`, y T-196 midió un
      documento cuya ganancia entera era formato de escritura— así que el número sería una
      adivinanza impresa junto a mediciones. El diálogo muestra el tamaño del archivo en
      disco, que sí sabe, y las cifras reales llegan apenas termina la compresión.

      **(2) El chooser de destino va DESPUÉS de correr, no antes.** Es el orden que este
      documento ya sugería ("ejecución en worker; al terminar, tamaño antes/después y
      chooser de destino") y resultó ser el único honesto: la garantía es "nunca más
      grande", no "siempre más chico", así que hasta no correr nadie sabe si hay una copia
      que valga la pena archivar. Un `NoGain` lo dice y no abre chooser.**

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
