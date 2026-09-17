# Ficha de batch B21 — Edición de contenido de página (texto/imágenes)

> Cambio de scope explícito (2026-08-12): el README declaraba "Edit PDF" (page-body
> editing) fuera del MVP, post-MVP junto con OCR y redacción. Este batch lo trae adentro,
> con alcance confirmado por el usuario: **edición in-place sin reflow** — reemplazar el
> contenido de un text run existente manteniendo su fuente/tamaño/posición (como el modo
> básico de Acrobat, no un motor de layout), más mover/redimensionar/reemplazar/borrar
> imágenes existentes e insertar texto/imagen nuevos como contenido de página real (no
> annotation-stamp). Reflow de párrafos queda explícitamente descartado — ni Acrobat lo
> hace así, y el proyecto no tiene ni necesita un motor de layout de texto.

**Dependencias:** B6 ✓ (crates core) únicamente. Independiente de los shells — paralelo a
B8/B9/B10/B13 y a B12/B20, igual criterio que ellos. Numeración: libre desde B21 en
adelante (B14 = iOS, B15–B19 reservados para firma criptográfica, B20 = formularios).

## Hecho clave del formato

El contenido de una página vive en uno o más **content streams** (`/Contents`, un stream
o un array de streams concatenados) — una secuencia de operadores de una pila mini-lenguaje.
Texto va entre `BT`/`ET`, posicionado por `Tm`/`Td`/`TD`, con la fuente activa seteada por
`Tf` (referencia a `/Resources /Font`) y los glyphs pintados por `Tj`/`TJ`/`'`/`"`. Imágenes
se pintan con `cm` (matriz de transformación) seguido de `Do` sobre un XObject en
`/Resources /XObject`. A diferencia de anotaciones y campos de formulario — que son
diccionarios independientes en `/Annots`/`/AcroForm`— el texto y las imágenes de la página
**no son objetos direccionables**: son bytes dentro de un stream, y editar uno sin tocar el
resto exige interpretar el stream completo para saber dónde empieza y termina cada operador.

## Decisiones de diseño

1. **Crate nuevo `core/pdf-edit`** (no extender pdf-manip ni pdf-annotate): aislamiento
   idéntico al de pdf-sign/pdf-form — un build futuro "solo visor" no debe arrastrar un
   parser/interpreter de content streams.
2. **El contenido de página NO se precarga en `Document`/`Page`** (a diferencia de
   `AnnotationSet`/`FormFieldSet`). Parsear el content stream de cada página al abrir el
   documento es trabajo desperdiciado cuando la mayoría de las sesiones nunca entran en
   modo edición de contenido — se carga on-demand, la primera vez que el shell entra en
   ese modo para una página dada (mismo criterio de lazy-loading que ya usa `pdf-render`
   para renderizar páginas).
3. **Reemplazo de texto exige que la fuente del run pueda representar el texto nuevo.**
   Fuentes simples (Standard-14, o embebidas con `/Encoding` WinAnsi/MacRoman/Differences
   resoluble) mapean cada carácter a un code byte; si el texto de reemplazo tiene un
   carácter sin code disponible, ESE reemplazo puntual se rechaza como `EncodingGap`
   ANTES de tocar el stream — nunca se escribe un glyph inválido. **Conservar una fuente
   compuesta Type0/CID al editar queda fuera de v1** (ver "Fuera de scope"): son el
   caso típico de fuentes embebidas subseteadas donde el mapeo de caracteres a glyphs no
   es trivial de extender sin re-subsetear la fuente, que es una categoría de trabajo
   aparte. La app puede sustituirla explícitamente por Helvetica/WinAnsi mediante un comando
   atómico; nunca lo hace en silencio. `editable` se evalúa por intento de reemplazo, no es un flag estático del run —
   un mismo run puede aceptar "café" y rechazar "日本語".
4. **Insertar texto/imagen nuevos es estructuralmente más simple que editar:** no hay
   encoding gap posible porque el recurso de fuente se crea nuevo (Standard-14 vía el
   mismo helper `/DR` que ya usa pdf-form) o el XObject de imagen se agrega desde cero.
   Se trata como una sub-feature de menor riesgo, separada de "editar un run existente".
5. **Todo cambio de contenido de página es ESTRUCTURAL** — a diferencia del relleno de
   formularios (no estructural, va incremental), reescribir un content stream siempre
   pasa por full rewrite. Si el documento tiene firmas (`pdf-sign`), quedan invalidadas —
   mismo criterio que ya aplica hoy a cualquier edición estructural, no una regla nueva.
   El shell debe superficiar esa advertencia antes de guardar.
6. **El re-render tras un commit deja de ser un caso de borde.** El WARNING de UX diferido
   en T-046/T-047 de B8 ("edits sin guardar requieren `save_to_bytes` + reabrir handle")
   era postergable para anotaciones (el overlay ya mostraba el resultado). Acá NO: editar
   texto/imagen de página cambia literalmente lo que pdfium renderiza, así que el ciclo
   save→reopen→re-render es el camino principal de la feature, no una optimización futura.

## Tareas

### Fase 1 — Modelo (`core/pdf-document`)
- [x] T-148 Módulo `content.rs`: `ContentItemId`, `TextRun {id, page, bbox: Rect
      (espacio de página, resultado de aplicar Tm + CTM), resource_font_name, font_kind:
      Standard14 | EmbeddedSimple | EmbeddedComposite, text: String}`, `ImageItem {id, page,
      bbox, resource_xobject_name}`, `PageContent {text_runs: Vec<TextRun>, images:
      Vec<ImageItem>}`. Espejo de `annotation.rs`/`form.rs` en estilo, pero **no** se cuelga
      de `Page` — ver T-149. [ContentModel]
      `Rect` se reusa de `annotation.rs` en vez de redefinirse. `PageContent` expone campos
      públicos (es un snapshot inmutable del parser, no estado mutable como `AnnotationSet`)
      más lookups `text_run(id)`/`image(id)` para que el shell resuelva por id lo que cruza
      el FFI. Los ids de texto e imagen se numeran por separado: el par (tipo, id) es lo que
      desambigua, y hay un test que lo fija.
- [x] T-149 API lazy: `pdf_edit::read_page_content(doc_bytes_or_ref, page: PageId) ->
      PageContent`, llamada por el shell al entrar en modo edición de esa página. `Document`
      no gana un campo `page_content` — decisión 2. [ContentModel]
      Arrancó a medias en Fase 1 (`Document` sin el campo, `PageContent` como tipo de
      retorno) y cerró con T-152, que es quien la puede implementar. Firma final:
      `read_page_content(&lopdf::Document, PageId) -> Result<PageContent, EditError>`.
- [x] T-150 Variantes nuevas de `Command` (`#[non_exhaustive]`, cero breaking change) **con
      inversa completa desde el día uno** (mismo estándar que fijó B20 para forms, no el gap
      que dejó B5 con anotaciones): `ReplaceTextRunContent {item: TextRun, after: String}`,
      `InsertTextRun(TextRun)`, `RemoveTextRun(TextRun)`, `InsertImage(ImageItem)`,
      `RemoveImage(ImageItem)`, `MoveImage {item: ImageItem, to: Rect}`, `ResizeImage {item:
      ImageItem, to: Rect}`, `ReplaceImageSource {item: ImageItem, before: Vec<u8>, after:
      Vec<u8>}` + apply/inverse + tests undo/redo (patrón `edit_log`). [ContentModel,
      UndoRedo]
      **Desvío del boceto original:** el boceto llevaba además `page` y un `before`/`from`
      explícitos. Se eliminaron por duplicar estado que el propio `item` ya carga —
      `item.page`, `item.text`, `item.bbox` — y dos fuentes de verdad que pueden discrepar
      son un bug esperando. `ReplaceImageSource` sí conserva `before`: los bytes NO son parte
      de `ImageItem`, así que ahí es la única forma de que el undo restaure la imagen previa.
      `apply` de las ocho es **inerte sobre `Document`** por la decisión 2: no hay contenido
      de página en el modelo que mutar, la entrada del log *es* la edición y `pdf-save` la
      replaya contra el archivo en el full rewrite (decisión 5). Tests cubren
      `inverse().inverse() == self` para las ocho, el round-trip undo/redo, que aplicar una
      no toque el modelo, y que un log mixto contenido+anotación se deshaga en orden.

### Fase 2 — Crate `core/pdf-edit`
- [x] T-151 Scaffold del crate (miembro del workspace; deps: pdf-document + lopdf), aislado
      del resto de core igual que pdf-sign/pdf-form. Módulos: `parse`, `encoding`, `edit`,
      `insert`. [infra]
      Se sumó `error.rs` (`EditError`, espejo de `AnnotateError`) y la dep `image` — la
      necesitan `insert_image`/`replace_image_source` para recalcular `/Width`/`/Height` y
      construir el `/SMask`. `parse` y `encoding` son módulos-directorio (`parse/lexer.rs`,
      `parse/matrix.rs`, `parse/interpreter.rs`, `encoding/tables.rs`) por tamaño: un
      `parse.rs` único habría sido un archivo de ~1000 líneas.
- [x] T-152 `parse.rs`: tokenizer + interpreter del content stream — trackea `q`/`Q`, `cm`
      compuesto y el text matrix (`Tm`/`Td`/`TD`) para computar el bbox de cada `TextRun`/
      `ImageItem` en espacio de página, correcto bajo rotación de página. Produce
      `PageContent`. [ContentParse]
      **Tokenizer propio, no `lopdf::content::Content::decode`:** el decode de lopdf no da
      offsets, y sin offsets T-154 no puede dejar el resto del stream byte a byte idéntico
      (un decode+encode reformatea números y espacios de toda la página). El lexer emite
      cada operación con el span de sus operandos y del operador.
      **Codec Flate propio (`parse/filter.rs`), tampoco el de lopdf.** Corrección
      post-review: `Stream::decompressed_content` no sirve para decidir si podemos tocar un
      stream. Dos motivos, y los dos terminan en un archivo corrupto. (1) Un inflate fallido
      **no se reporta**: `decompress_zlib` loguea, reintenta deflate crudo y devuelve `Ok`
      con lo que haya salido — un prefijo. Ese prefijo parece contenido de página, y al
      editar se escribe de vuelta como la página entera: truncada. (2) `/Filter` **no se
      dereferencia**, y puede ser perfectamente una referencia indirecta; un lector que sólo
      matchea el objeto directo concluye "sin filtro" y entrega bytes comprimidos como
      operadores.
      La regla ahora es: **si no podemos probar que entendimos el stream entero, no lo
      tocamos.** El inflate exige `Status::StreamEnd` — no "salió algún byte" —, `/Filter` se
      resuelve a través de indirección (también dentro del array), y se rechazan cadenas de
      filtros, filtros distintos de `FlateDecode` y cualquier `/DecodeParms`, con
      `UnsupportedContentStreamFilter` en vez de `UndecodableContentStream` porque significan
      cosas distintas para quien lee el error. Salida vacía con `StreamEnd` es una página
      vacía legítima, no un fallo — la heurística anterior ("comprimido no vacío que decodifica
      a vacío") tenía justamente ese falso positivo.
      El encode también es propio: `Stream::compress` es best-effort (declina si no ahorra
      más de 19 bytes), así que una página chica que llegó comprimida volvía en texto plano
      con el `/Filter` borrado. Y `PageStream` guarda si el stream venía filtrado, en vez de
      volver a mirar el diccionario al escribir: la lectura que resolvió `/Filter` es la que
      sabe, y dos lecturas del mismo campo pueden discrepar.
      **El fallback a deflate crudo sólo corre si no hay header zlib válido.** Existe para
      productores que se saltearon los dos bytes de header, y esa es la única situación en la
      que es una explicación plausible. Reintentar un zlib *dañado* como crudo lee sus bytes de
      header como el arranque de un stored block, y un stored block cuyo LEN/NLEN casualmente
      cierra entrega bytes que pueden llegar hasta un fin de stream sin significar nada —
      deflate crudo no trae Adler-32 para atajar eso. El chequeo de header (`has_zlib_header`)
      es lo único que separa una página dañada de una reescritura que parece sana.
      **El techo de decodificado es por página, no por stream** (`MAX_PAGE_CONTENT_BYTES`,
      64 MB). `/Contents` es un array de largo arbitrario y nada le impide repetir la misma
      referencia, así que un límite por stream se multiplica por la cantidad de entradas y no
      acota nada; `page_streams` reparte un presupuesto único entre los decodes. Se reporta
      como `PageContentTooLarge` y no como stream indecodificable, porque el archivo no está
      roto: el número es nuestro.
      **El nivel de compresión es `default` (6), no `best` (9)**, porque el encode no corre una
      vez por guardado sino una vez por *comando*: `replay_content_edits` recorre el log de a
      una entrada y cada una relee la página entera y la vuelve a escribir. Diez ediciones en
      una página son diez round trips completos del stream.
      **`/Rotate` NO se aplica al bbox:** es una instrucción de visor, no una transformación
      del contenido, y las shells ya la aplican para `Annotation.rect`. Aplicarla acá haría
      doble rotación. Hay un test que fija la equivalencia.
      Cierra también **T-149**: `read_page_content(&lopdf::Document, PageId)` es la API lazy,
      sin cache en `Document`.
- [x] T-153 `encoding.rs`: mapea el texto de reemplazo/inserción a codes contra el
      `/Encoding` de la fuente del run (Standard-14/WinAnsi/MacRoman/Differences); devuelve
      `EncodingGap` tipado por carácter no representable en vez de escribir un glyph
      inválido. Fuentes Type0/CID: el reemplazo que conserva la fuente se rechaza; la
      sustitución explícita usa la fuente estándar de inserción (decisión 3). [ContentEncoding]
      Asimetría deliberada: **decodificar** (codes→texto) es best-effort y devuelve U+FFFD
      para lo que no sabe mapear, así el run igual se ve; **codificar** (texto→codes) es
      estricto y falla. Un code sin mapeo conocido produce un `EncodingGap`, nunca un glyph
      inventado — ver "Cobertura de encoding en v1" abajo.
- [x] T-154 `edit.rs`: reescritura quirúrgica del content stream — `replace_text_run`
      reemplaza SOLO el operando de `Tj`/`TJ` del run targeteado, resto del stream byte a
      byte idéntico; `remove_item` elimina el rango de operadores del item; `replace_
      image_source` reemplaza el stream del XObject (`/Width`/`/Height`/`/Filter`
      recalculados) manteniendo el mismo nombre de recurso. [ContentEdit]
      **Desvío en move/resize:** el boceto decía "reescriben el `cm` inmediatamente anterior
      al `Do`". Eso es incorrecto cuando ese `cm` está compuesto de varios o lo comparten
      dos `Do` — reescribirlo mueve también la otra imagen. En vez de eso se reemplaza la
      operación `Do` por `q <corrección> cm /Name Do Q`, donde la corrección es
      `M_destino × inverse(CTM_en_el_Do)`. Siempre correcta, y el `q`/`Q` deja intacto el
      estado gráfico que hereda el resto de la página. Hay un test con dos `Do` bajo el
      mismo `cm` que falla con el enfoque del boceto.
      **`remove_text_run` no deja un hueco vacío:** reemplaza la operación por un ajuste
      `TJ` que no pinta nada y avanza el text matrix exactamente lo que avanzaban los glyphs
      borrados. Sin eso, el texto siguiente de la línea se corre a la izquierda — o sea,
      reflow, que está fuera de scope por decisión.
- [x] T-155 `insert.rs`: agrega texto/imagen nuevos al final del content stream de la
      página (o a un stream nuevo si `/Contents` es array), registrando el recurso en
      `/Resources` — fuente Standard-14, o un XObject de imagen nuevo. [ContentInsert]
      No hay helper `/DR` de pdf-form que reusar: **B20 todavía no está implementado**, en
      `core/pdf-document` no existe `form.rs`. La fuente Standard-14 (Helvetica/WinAnsi) se
      registra acá; cuando B20 llegue, vale unificar.
      Lo insertado se coloca cancelando el CTM que dejan los streams de la página
      (`end_ctm`), así una página que termina dentro de un `q ... cm` desbalanceado igual
      recibe el contenido en coordenadas de página. El `/Resources` de la página se
      materializa como propio antes de escribirlo, para no agregarle el recurso a las
      páginas hermanas que compartían el diccionario heredado.

### Fase 3 — Serialización (`core/pdf-save`)
- [x] T-156 `content.rs`: rutea cualquier `Command` de contenido de
      página SIEMPRE por full rewrite en `strategy.rs` (decisión 5); si el documento tiene
      firmas existentes, expone la advertencia de invalidación al llamador — no bloquea el
      guardado, igual criterio que ya rige para otras ediciones estructurales. [ContentSave,
      FirmaCripto]
      **No va sobre `ObjectSink`**, contra lo que decía el boceto. El sink existe para
      compartir la escritura de anotaciones entre los dos writers; el contenido de página
      tiene un solo writer por construcción (decisión 5), así que una abstracción con una
      única implementación sería una capa puesta para verse simétrica — justo lo que el
      protocolo de AGENTS.md pide no hacer. `content.rs` trabaja sobre `lopdf::Document`,
      que es lo que el full rewrite ya tiene en la mano.
      La advertencia se expone como `strategy::will_invalidate_signatures(input) ->
      Result<bool>`: el shell la consulta antes de escribir y decide el usuario. Detecta
      firmas por `/Type /Sig`, `/FT /Sig` y por `/AcroForm /SigFlags` con el bit
      SignaturesExist — escanea objetos además de caminar `/AcroForm /Fields` porque un
      árbol de formulario dañado igual puede llevar una firma, y no avisar es peor que
      avisar de más.
      **Corrección post-review:** exponer la advertencia no alcanzaba. `will_invalidate_signatures`
      era una consulta opcional que ningún llamador hacía — ni `save_document`, ni
      `pdf_ffi::save_to_bytes`, ni ningún shell —, así que en la práctica el usuario perdía la
      firma sin enterarse. Ahora `SaveInput` lleva `signatures: SignatureAcknowledgement`
      (`Unacknowledged` por defecto) y `save_document` devuelve
      `SaveError::SignaturesWouldBeInvalidated` si el guardado rompe una firma existente y el
      llamador no declaró que ya avisó. No es un bloqueo: **no hay forma de conservar la firma**
      en un rewrite, así que negarse a secas volvería inguardables los documentos firmados. Lo
      que el tipo garantiza es que la pregunta se haga; la respuesta sigue siendo del usuario.
      El shell GTK4 la hace con un `AlertDialog` y reenvía el guardado con
      `ProceedAndInvalidate`.
      El replay corre **antes** de anotaciones, y **después** de que `page_object_ids` resuelva
      el mapa `PageId -> ObjectId` contra el documento ya replayado: los comandos de contenido
      se localizan reparseando los streams de la página, y un `PageId` es identidad estable, no
      posición. Resolverlo posicionalmente después de un borrado o reordenamiento de páginas
      aplicaba la edición a la página equivocada — por eso toda la API de edición de `pdf-edit`
      toma `ObjectId` y no `PageId`.
- [x] T-157 Wiring: `bridge.rs` NO puebla contenido de página al abrir (decisión 2); expone
      el acceso lazy delegando directo a `pdf_edit::read_page_content` sobre el lopdf backing
      doc. [ContentSave]
      `pdf_save::read_page_content(&LopdfDocument, PageId)`. Hay un test de integración que
      fija lo que más fácil se rompe en silencio: que los `PageId` que reparte
      `document_from_lopdf` sean los mismos que toma la lectura lazy.

### Correcciones a Fase 1 y Fase 2 que destapó Fase 3

Escribir el writer expuso dos huecos que ninguna de las dos fases anteriores podía ver sola:

1. **`Command::InsertImage(ImageItem)` no llevaba los bytes de la imagen.** `ImageItem` no
   los tiene a propósito, así que el writer no tenía con qué construir el XObject — la
   feature "insertar una imagen nueva" era inejecutable tal como estaba modelada. Ahora es
   `InsertImage { item, source: Option<Vec<u8>> }`, y `RemoveImage` lleva el mismo `source`
   para que `inverse().inverse()` siga siendo identidad: un redo de la inserción necesita
   esos bytes. `source: None` significa "el recurso ya está registrado, solo falta volver a
   pintarlo", que es lo que hace deshacer un borrado.
2. **Los ids son posiciones en un parse, y un borrado renumera la página.** Con resolución
   solo por id, un save con dos comandos donde el primero borra fallaba con `ItemNotFound`
   en el segundo. Ahora `pdf-edit` resuelve por la identidad propia del item — texto +
   fuente + posición para runs, nombre de recurso + posición para imágenes — usando el id
   apenas como atajo. Dos items idénticos en los tres campos son indistinguibles y gana el
   primero; está documentado.

### Fase 4 — FFI (`core/pdf-ffi`)
- [x] T-158 `FfiContentTextRun`/`FfiContentImageItem`/`FfiPageContent`/`FfiFontKind` en
      types.rs; `FfiEditCommand` gana `ReplaceTextRunContent/InsertTextRun/RemoveTextRun/
      InsertImage/RemoveImage/MoveImage/ResizeImage/ReplaceImageSource`;
      `DocumentHandle::read_page_content(page) -> FfiPageContent` (carga lazy bajo demanda
      del shell). Smoke test: editar un run Standard-14 → save_to_bytes → reabrir →
      read_page_content devuelve el texto nuevo. [ContentFFI]
      **Naming:** no `FfiTextRun`/`FfiImageItem` como decía el boceto — esos nombres ya
      existen en `types.rs` para el texto *extraído* por pdfium (`text_runs`/`search`), un
      concepto distinto del texto *parseado* del content stream que edita `pdf-edit`.
      Reusarlos habría hecho colisionar dos tipos con forma y significado distintos bajo un
      mismo nombre. Se usó `FfiContentTextRun`/`FfiContentImageItem` en su lugar.
      **Error mapping:** `pdf_edit::EditError::EncodingGap` se promovió a variante propia
      de `FfiError` (`character`/`resource_font_name`, `char` cruza como `String` — UniFFI
      0.31 no tiene un tipo `char`) porque es el único caso que un shell necesita distinguir
      en UI (T-161: mostrar el run como no editable con explicación). El resto de
      `EditError` (`ItemNotFound`, `PageNotFound`, `MalformedContent`, etc.) cae en
      `FfiError::Internal`, mismo criterio que ya aplica `AnnotateError`/`ManipError`: solo
      se tipa lo que el llamador puede accionar distinto.
      **`apply_edit` no valida el item contra el stream real** — coherente con la decisión 5
      (el log de las ocho variantes es inerte sobre `Document`; `pdf-edit` resuelve el item
      recién en el replay de `save_to_bytes`). Un `ReplaceTextRunContent` con un item
      obsoleto (releído después de que el documento cambió) es aceptado por `apply_edit` y
      solo falla al guardar — verificado con un test dedicado, para que este comportamiento
      no se lea como un bug si alguien lo redescubre.
      Requirió agregar `pdf-edit` como dependencia directa de `pdf-ffi` (antes solo llegaba
      transitivamente vía `pdf-save`) — necesario para nombrar `pdf_edit::EditError` en el
      `impl From<...> for FfiError`, mismo patrón que la dependencia directa a `pdf-annotate`
      que ya existía por la misma razón.

### Fase 5 — Fixtures e interop
- [x] T-159 Fixtures: un PDF con texto en fuente Standard-14 (editable), uno con fuente
      embebida subset (para probar el camino `EncodingGap` → run no editable para cierto
      texto), uno con imagen (para move/resize/replace). Al menos uno generado por
      herramienta externa (mismo criterio que T-144 de B20). [ContentFixtures]
      **Standard-14:** sin fixture nuevo — `gen_fixtures::build_multi_line_page_document`
      ya era Helvetica/Standard-14 y ya lo usaba de punta a punta el resto de
      `content_edit_roundtrip.rs`; agregar uno redundante habría sido la abstracción
      innecesaria que AGENTS.md pide evitar.
      **Embebida subset (externo):** `tests/fixtures/content-edit/reportlab_embedded_subset.pdf`,
      generado una sola vez por `generate_reportlab_embedded_subset.py` (reportlab +
      `Vera.ttf`, la fuente que reportlab empaqueta bajo la licencia Bitstream Vera, que
      permite explícitamente embeberla en un documento y redistribuir ese documento — el
      script documenta la licencia y cómo regenerar el fixture). reportlab embebe
      `Vera.ttf` como fuente `/Subtype /TrueType` simple con tag de subset en `/BaseFont`
      y sin `/Encoding` — `pdf-edit` clasifica esto como `FontKind::EmbeddedSimple` y cae
      en su tabla por defecto (cobertura ASCII), reproduciendo el ejemplo de la decisión 3
      contra una fuente embebida real: "New Words" se reemplaza, "café" no.
      **Corrección durante la generación:** el primer intento salió con
      `/Filter [/ASCII85Decode /FlateDecode]` en el content stream — el default de
      reportlab —, que el codec estricto de T-152 rechaza a propósito (cadena de filtros).
      Es el comportamiento correcto del codec, no un bug del fixture; se regeneró con
      `pageCompression=0` para dejar el content stream sin filtrar, ya que lo que este
      fixture necesita probar es la resolución de `/Encoding`, no el rechazo de cadenas de
      filtros (que ya tiene su propia cobertura en las unit tests de T-152).
      **Imagen:** `gen_fixtures::content_edit::build_image_page_document()` (nuevo módulo
      `content_edit.rs` en `gen-fixtures`, mismo patrón que `large::build_large_document`
      pero a escala de fixture, no de perf) — una imagen 4x4 pintada en un rect conocido
      (100, 600, 80x40pt) para que los tests de move/resize de T-160 puedan aserear ancho y
      alto por separado.
      **Tests de "fixture válido"** (no el round-trip completo, eso es T-160) agregados a
      `core/pdf-save/tests/content_edit_roundtrip.rs`: el fixture externo parsea como
      `EmbeddedSimple` y su texto es el esperado; un reemplazo ASCII sobre ese run
      sobrevive un `save_document` real y uno no-ASCII falla con
      `SaveError::Edit(EditError::EncodingGap)`; el fixture de imagen expone exactamente
      una `ImageItem` en el rect documentado.
- [x] T-160 Tests round-trip (editar → save full-rewrite → reabrir → el resto del contenido
      es byte-idéntico donde no fue tocado) + validador Python independiente (pypdf) que
      extrae texto del output y confirma que el run editado cambió. [Parity]
      **(2026-08-13 — completo: tests de integración en Rust sobre replace/move/resize/
      replace de texto e imagen vía el full-rewrite de pdf-save, más un productor `#[ignore]`
      que le pasa el PDF resultante a un validador pypdf hash-pineado (`tools/pypdf-validation`)
      y un job de CI offline dedicado en `core.yml`. Solo tests/tooling — cero cambios de
      producción, API, FFI o UI.)**

### Fase 6 — Docs
- [x] T-166 README: mover "Edit PDF" de `🔮 Planned` al roadmap activo; enlazar esta ficha
      (hecho en el mismo cambio que crea este documento).

### Fase 7 — Inline images

Las imágenes `BI`..`EI` dejan de ser un agujero del modelo. Ver la sección
"Inline images: medir en vez de adivinar" más abajo para el porqué de cada decisión.

- [x] T-200 `core/pdf-edit/src/parse/inline.rs`: parsear el diccionario del `BI` (pares
      sueltos entre el operador y `ID`, no un `<< >>`) y **medir** dónde terminan las
      muestras — `/L`, aritmética exacta cuando no hay filtro, y recién ahí el scan de
      `EI`. El lexer deja de buscar `ID` a ojo, que un valor que deletree el marcador
      (`/CS /IDSpot`) cortaba antes de tiempo.
- [x] T-201 `ImageSource` en `pdf-document`: una imagen se pinta con `Do` contra un
      recurso o inline, y el modelo lo dice en vez de cargar un nombre vacío. El
      interpreter reporta las dos en una sola numeración; `page_image_placements` (el
      inventario de compresión, T-193) deja afuera la inline porque no hay objeto que
      tocar; la FFI la pasa como nombre opcional.
- [x] T-202 Mover/redimensionar/borrar una inline image: las tres son reescrituras del
      span, y las muestras se copian byte a byte. Lo que sí necesita un recurso —
      reemplazar la fuente, leerla de vuelta para el undo, insertar una nueva — se rechaza
      con `EditError::InlineImageNotSupported`, ruidoso y antes de tocar nada.
- [x] T-203 `replace_image_source`/`image_source_bytes` para inline images: leer las
      muestras del stream y devolverlas como PNG/JPEG para el `before` del undo, y
      reescribir el `BI`..`EI` como XObject de página propia + `Do` al reemplazar (una
      inline image no puede llevar `/SMask`, así que el reemplazo con alpha no puede
      quedarse inline). La lectura pasa por `InlineImage::as_image_stream`, que expande el
      diccionario abreviado al de un XObject y deja que el lector que ya existía haga el
      resto — la inline no aprende un vocabulario nuevo, se traduce al que había.
- [x] T-204 (dep T-203) UI: que el shell Linux las trate como cualquier otra imagen, y
      que el rechazo de lo que no se puede se vea como explicación, no como fallo.
      **(2026-09-17 — completo.)** La primera mitad ya era cierta *por construcción*:
      ninguna de las cinco operaciones de imagen del shell (seleccionar, mover,
      redimensionar, borrar, reemplazar) lee jamás el nombre del recurso — hit-test y
      drag trabajan sobre el `bbox`, y las validaciones llaman a `pdf-edit` con el
      `ImageItem` entero. Lo que no existía era una *garantía*: cero tests del shell
      sobre una página con inline image, así que la uniformidad vivía en el comentario
      de un módulo. Ahora es un gate — `build_inline_image_page_document` pinta una
      inline y un XObject en la misma página, y los tests corren las cinco validaciones
      contra las dos y comparan resultados.
      **El rechazo.** El único alcanzable desde el shell es
      `EditError::ImageSourceNotRecoverable` al leer los bytes actuales para el `before`
      del undo — y no es una propiedad de la inline sino de la *codificación* (una
      `Indexed`, un `/Decode`, 16 bits, un JPEG con `/SMask`; en una inline, un `/CS`
      que nombra un recurso de página). Aparecía **después** del file picker, como la
      frase cruda del core en la barra de estado: al usuario se le pedía elegir un
      archivo de reemplazo para recién entonces decirle que ningún reemplazo era
      posible. Eso es un rechazo entregado como fallo. Ahora la lectura se hace *antes*
      de abrir el diálogo (`image::replace_readback_refusal`), el resultado se recuerda
      en `SelectedImage::replace_refused`, y el botón "Replace image" queda insensible
      con `panel::IMAGE_SELECTED_NO_REPLACE` al lado explicando por qué — mientras
      Delete, mover y redimensionar siguen vivos, porque ninguno de los tres lee la
      imagen de vuelta.
      **Qué se recuerda y qué no**: `command::is_unreplaceable` acepta ese error y
      ninguno más. Un archivo de reemplazo que no decodifica, un item viejo o una imagen
      con una edición ya encolada son respuestas sobre *este intento*, y latchear
      cualquiera de ellas dejaría un control muerto detrás de una frase falsa.
      **Costo**: una decodificación extra en el camino feliz, pagada por click en un
      botón explícito — no por selección ni por frame. Los bytes del pre-chequeo se
      tiran: la selección puede cambiar mientras el diálogo está abierto, así que el
      `before` que se graba tiene que venir de una lectura tomada en el commit.
      **Seguimiento, mismo día:** el otro rechazo del mismo botón —"esta imagen ya
      tiene una edición pendiente"— tenía la misma forma y la misma cura, y encima es
      gratis de contestar (`command::image_already_edited` es un scan del `EditLog`, sin
      decode). El estado de la tarjeta dejó de vivir en tres expresiones distintas de la
      misma pregunta y pasó a ser un valor: `content_edit::ImageControls`
      (`Nothing`/`Ready`/`NoReplace`/`PendingEdit`), resuelto por `image_controls` y
      gastado en widgets por `update_content_edit_controls`. `PendingEdit` apaga los dos
      botones —una edición encolada refuta las cuatro operaciones, no sólo el reemplazo—
      y **gana** sobre `NoReplace` cuando valen las dos: es la respuesta más ancha y es
      la única que el usuario puede resolver, guardando. A diferencia de
      `replace_refused`, **no se cachea**: `write::preview::edits` lleva el
      `document_model` —y con él el `EditLog`— a través del refresh, así que un `true`
      recordado sobrevivría a su causa. Por eso mismo el handler de undo/redo
      (`annotations::command::history`) ahora llama a `update_content_edit_controls`
      junto a sus dos hermanas, con el argumento que ya estaba escrito ahí para ellas:
      el refresh de un content edit re-corre esto cuando aterriza, pero puede no
      aterrizar, y un refresh fallido no puede dejar la tarjeta rechazando una imagen que
      el log acaba de liberar.

## Tareas de UI (agregadas a la ficha de B8, docs/batches-b8-b13.md)

- [x] T-161 (dep B21) Modo edición de contenido en el canvas: click sobre un text run
      existente abre un editor inline que preserva fuente/tamaño/posición; los runs no
      editables por `EncodingGap` se muestran distinguibles con explicación al usuario —
      nunca un fallo silencioso ni un intento que corrompa el stream. [ContentEdit]
      **(completo, ver la entrada extendida en `docs/batches-b8-b13.md`.)**
- [x] T-162 (dep B21) Imágenes de página real: seleccionar/mover/redimensionar con
      handles/reemplazar (file picker)/borrar imágenes existentes — distinto del stamp de
      anotación que ya existe (T-047). [ContentEdit]
      **(completo, ver la entrada extendida en `docs/batches-b8-b13.md`.)**
- [x] T-163 (dep B21) Insertar texto/imagen nuevos como contenido de página real; y el
      ciclo save→reopen→re-render obligatorio tras cualquier commit de edición de contenido
      (decisión 6) — acá el WARNING de UX diferido en T-046/T-047 deja de ser diferible.
      [ContentEdit]
      **(2026-08-17 — completo, ver la entrada extendida en `docs/batches-b8-b13.md`.)**

## Tareas de UI en otros shells

- [x] T-177 (dep B21) Retipear un text run en el shell Windows (WinUI), a través de la FFI en
      vez de linkear `pdf-edit`. Lo que ese camino exigía del core — validar antes de grabar,
      amendar el retipeo repetido, refrescar el preview y gatear el permiso de contenido —
      vive en `pdf-save`/`pdf-ffi`, no en C#, así que macOS/Android/iOS lo heredan.
      **(2026-08-19 — completo para texto; ver la entrada extendida en
      `docs/batches-b8-b13.md`, B10.)**

## Criterios de aceptación

- Editar un text run en fuente Standard-14/simple con glyphs representables → el PDF
  resultante muestra el texto nuevo en Acrobat/Preview con la misma fuente/tamaño/posición.
- Un run cuya fuente simple no puede representar el texto de reemplazo se detecta ANTES de
  escribir. Un run Type0/CID se sustituye solo tras confirmación explícita, usando una fuente
  estándar compatible. Ninguno de los dos caminos corrompe el content stream.
- Mover/redimensionar/reemplazar/borrar una imagen existente produce un PDF válido; el
  resto de la página es byte-idéntico donde no fue tocado.
- Insertar texto/imagen nuevos los agrega como contenido de página real (visible al
  extraer texto/imagen con herramientas externas), no como annotation.
- Cualquier commit de edición de contenido pasa por full rewrite; si invalida firmas
  previas, el shell lo advierte antes de guardar.
- El visor re-renderiza el resultado real tras cada commit (no un overlay simulado).
- Undo/redo completo desde el día uno para las ocho variantes de `Command` nuevas.

## Cobertura de encoding en v1 (T-153)

Lo que el mapeo de codes cubre hoy, y lo que rechaza. La dirección conservadora es
deliberada: una tabla incompleta cuesta una edición rechazada, una tabla equivocada cuesta
un documento corrupto — y la decisión 3 prohíbe exactamente lo segundo.

| Encoding | Cobertura | Efecto de lo no cubierto |
| --- | --- | --- |
| WinAnsiEncoding | **Completo y exacto** — ASCII, el bloque Windows-1252 `0x80-0x9F` y Latin-1 `0xA0-0xFF` | — |
| StandardEncoding (default) | **Completo** — ASCII con las dos excepciones de comillas (`STANDARD_ASCII_OVERRIDES`) + toda la mitad alta del Annex D (`STANDARD_HIGH`) | los codes que el encoding deja sin usar siguen siendo `EncodingGap` al escribir, U+FFFD al leer |
| MacRomanEncoding | **Completo salvo dos codes** — ASCII + `MAC_ROMAN_HIGH`, que suma a la columna `MAC` del Annex D los slots no-latinos de `ROMAN.TXT` (`≠ ∞ ≤ ≥ ∂ ∑ ∏ π ∫ Ω √ ≈ Δ ◊`) | `0xDB` y `0xF0` quedan sin mapear a propósito, ver abajo |
| `/Differences` | Nombres de glyph del set latino del Annex D + los no-latinos de MacRoman + formas `uniXXXX`/`uXXXX` + letras sueltas | nombre desconocido ⇒ el code queda sin mapear |

Las tres tablas viven en `core/pdf-edit/src/encoding/tables.rs` y un test las mantiene
ordenadas, sin codes repetidos y con un nombre de glyph para cada carácter que alguna de
ellas pinta — de modo que una fuente que reafirme su propio encoding vía `/Differences` no
pierda codes que ya tenía.

**Los dos codes que MacRoman deja sin mapear** son los únicos donde dos autoridades se
contradicen, y la política del módulo es rechazar, no elegir:

- `0xDB` — el Annex D lo llama `currency` (U+00A4); Mac OS Roman pone ahí el Euro (U+20AC).
- `0xF0` — el logo de Apple, en U+F8FF, área de uso privado.

Lo que sigue **fuera** de las tablas es lo que ninguna tabla arregla: Type0/CID conservando
la fuente (ver "Fuera de scope").

## Form XObjects: copia propia de la página al escribir

El texto y las imágenes pintados dentro de un `/Subtype /Form` **sí** se reportan y se
editan. El interpreter desciende al form y cada item se lleva su `form_path`: la cadena de
invocaciones `Do` que va de la página hasta el stream donde vive.

Escribir ahí no puede tocar el objeto compartido —el stream de un form lo puede invocar
cualquier cantidad de páginas, y no existe forma de editarlo "para una sola"—, así que no se
edita: `edit::own_form_path` **copia cada paso del camino** y liga la copia en la copia de su
llamador. La página que edita pasa a alcanzar sus copias; todas las demás siguen alcanzando
los originales. Lo mismo vale para el `/Resources` de la página, que se desprende de
cualquier diccionario indirecto compartido antes de reescribir la ligadura.

Detalles que la implementación fija:

- Una copia que no tenía `/Resources` propio recibe el de su llamador —el mismo diccionario
  contra el que el parse resolvió sus nombres—. Sin eso, el primer recurso que se registre en
  la copia sería el único nombre que el form podría seguir viendo.
- Los nombres se resuelven en el **scope** del item, no en la página: un form que redefine
  `/F1` se edita contra *su* fuente. `edit::scope_resources` camina el `form_path` con la
  misma regla que el interpreter (el `/Resources` propio reemplaza al del llamador, no se
  mezcla con él).
- La invariante 1 sigue en pie: todo lo que puede rechazar la edición corre **antes** de que
  se copie nada. Un reemplazo que la fuente no puede codificar no deja copias a medio hacer.
- **Repetir una edición dentro del mismo form lo copia de nuevo.** La copia anterior queda
  sin referenciar en lugar de reusarse: decidir que una copia es exclusiva de esta página
  significa probar que ningún otro objeto la alcanza, y eso es un recorrido de alcanzabilidad
  de todo el documento que este crate no tiene. El prune de `pdf-compress` las junta a la
  salida.

## Inline images: medir en vez de adivinar

Una inline image lleva sus muestras **dentro** del content stream: `BI`, el diccionario,
`ID`, binario crudo, `EI`. No hay objeto, no hay nombre, y hasta PDF 2.0 no hay largo
declarado — así que encontrar dónde termina es todo el problema, y el scan del próximo
`EI` que usan los visores es una heurística que se equivoca sobre sus propias muestras.

`core/pdf-edit/src/parse/inline.rs` deriva el final y sólo adivina cuando no puede:

1. `/L` (`/Length`), y sólo si efectivamente hay un `EI` ahí — un `/L` mentiroso no puede
   cortar la imagen en silencio.
2. Sin filtro, aritmética exacta: `ceil(W × BPC × componentes / 8) × H`. Cero heurística,
   y es el caso común de las imágenes chiquitas para las que la codificación inline existe.
3. Recién entonces, el scan de `EI` delimitado por whitespace — el largo de la salida de un
   filtro no se deriva del diccionario, así que no hay nada mejor disponible.

Un espacio de color nombrado contra `/Resources /ColorSpace` cae al scan a propósito: sus
componentes se saben, pero no desde el stream, y un conteo equivocado daría un final
confiadamente incorrecto.

Lo que se puede hacer con una, y lo que no:

| Operación | Inline | Por qué |
| --- | --- | --- |
| Mover / redimensionar / borrar | ✅ | Son reescrituras del span; las muestras se copian byte a byte, nunca se re-codifican |
| Aparecer en `PageContent` con su caja | ✅ | Pinta el mismo cuadrado unitario que un `Do` |
| Leerla de vuelta para el undo | ✅ | Su diccionario abreviado se traduce al de un XObject y lo lee el mismo lector (T-203) |
| Reemplazar la fuente | ✅ — deja de ser inline | Las muestras *son* la operación: el reemplazo se registra como XObject de la página y el span pasa a ser un `Do` (T-203) |
| Insertarla nueva | ❌ | Lo que se agrega es un `Do`, y un `Do` necesita un nombre |
| Entrar al inventario de compresión (T-193) | ❌ | `pdf-compress` reemplaza objetos; acá no hay objeto |

El modelo lo dice con `ImageSource::{Resource, Inline}` en vez de un nombre vacío, y a
diferencia de `FontKind` **no** es `#[non_exhaustive]`: el formato define exactamente dos
formas de pintar una imagen, y quien escribe tiene que manejar las dos.

Reemplazar es la única edición que no puede dejarla inline, y no por comodidad: las
muestras **son** la operación, así que reescribirlas es reescribir el `BI … EI` entero, y
la codificación inline no puede cargar lo que produce nuestro encoder — no tiene `/SMask`,
así que cualquier reemplazo con alpha saldría opaco, y sus abreviaturas de filtro no
cubren la cadena con predictor PNG que escribe `insert::image_xobject`. Entonces el
reemplazo se registra como XObject sobre el *scope que pintaba la inline* (página o copia
propia del form), con un nombre recién acuñado porque la inline no traía ninguno, y el
span pasa a ser un `/Nombre Do`. Ese `Do` pinta el mismo cuadrado unitario, bajo el mismo
CTM, en el mismo punto del stream: cambia el dibujo y nada más.


## Otras limitaciones conocidas de Fase 2

- **Los forms no anidan más de `MAX_FORM_DEPTH` (32) niveles.** El descenso corta ciclos
  (un form que ya está activo no se vuelve a entrar) pero eso no acota una cadena larga de
  forms *distintos*: ~600 niveles desbordan la pila de un thread de test de 2 MiB, y un
  desborde es un abort del proceso, no un `Err` que alguien pueda manejar. El presupuesto de
  `MAX_PAGE_CONTENT_BYTES` tampoco lo acota, porque cada nivel cuesta los ~7 bytes de un
  `/N Do`. Por eso el tope es explícito y el nivel 33 se rechaza con
  `EditError::FormNestingTooDeep` **antes** de bajar: mismo techo que el
  `MAX_INHERITANCE_DEPTH` de la cadena `/Parent`, y generoso para cualquier archivo real.
  Rechazo ruidoso, nunca contenido truncado en silencio: una página que reportara sólo los
  primeros 32 niveles escondería items que el usuario ve pintados.
- **Los dos walkers ya no discrepan sobre los forms.** `parse::interpreter::interpret`
  desciende y `parse::placement::page_image_placements` lo hereda, así que una imagen
  dentro de un form es visible tanto para editar como para el inventario de compresión
  (T-193). Lo que lo hacía divergir no era el descenso sino la **resolución del nombre**:
  el placement resolvía `/Im0` contra los recursos de la *página* después de haber entrado
  a un form que redefine ese nombre, y devolvía el objeto equivocado con la matriz correcta.
  Ahora el id lo resuelve el intérprete en el scope que pintó (`LocatedImage::xobject`) y el
  módulo de placements sólo mide. Un recurso que no es objeto indirecto sigue afuera: no hay
  id que un llamador pueda tocar.
- **El ancho del bbox de un run es aproximado** cuando la fuente no trae `/Widths` (caso
  típico de las Standard-14): se asume medio em por glyph. El alto usa 0.75/-0.25 em en vez
  de leer `/Ascent`/`/Descent`. Afecta la precisión del hit-test en la UI, nunca lo que se
  escribe.
- **Editar dos veces el mismo item sin releer entre medio falla** con `ItemNotFound`. El
  segundo comando lleva la identidad vieja (el texto o la posición anterior), que ya no
  existe en la página. Falla ruidosamente, nunca edita el item equivocado. El ciclo
  save→reopen→re-render de la decisión 6 —que T-163 implementa— es el flujo previsto y lo
  evita; el shell arma el segundo comando a partir de una lectura fresca.

## Fuera de scope (v1)

Reflow de párrafos / wrap de texto (decisión de producto confirmada: sin motor de layout) ·
edición conservando una fuente compuesta Type0/CID (la sustitución explícita sí está
soportada) · cambiar fuente o tamaño de un run existente (eso es "restyle", no "edit"; candidato a fase 2 igual que el flatten de
formularios) · OCR de contenido escaneado · redacción — estos dos últimos siguen post-MVP
per README.

## Orden de ejecución

Fase 1 → 2 → 3 → 4 lineal (cada fase compila y testea sola, TDD estricto — igual criterio
que B20). El fixture con fuente embebida subset (T-159) se necesita para cerrar T-153
(camino `EncodingGap`). La UI de B8 (T-161–163) arranca cuando T-158 (FFI) esté listo — B8
bypasea FFI para el resto del shell, pero igual consume el mismo modelo lazy de T-149/T-152
para no reimplementar el parser de content streams.
