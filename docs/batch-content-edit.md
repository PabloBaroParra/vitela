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
   ANTES de tocar el stream — nunca se escribe un glyph inválido. **Fuentes compuestas
   Type0/CID quedan fuera de la edición de texto en v1** (ver "Fuera de scope"): son el
   caso típico de fuentes embebidas subseteadas donde el mapeo de caracteres a glyphs no
   es trivial de extender sin re-subsetear la fuente, que es una categoría de trabajo
   aparte. `editable` se evalúa por intento de reemplazo, no es un flag estático del run —
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
- [ ] T-148 Módulo `content.rs`: `ContentItemId`, `TextRun {id, page, bbox: Rect
      (espacio de página, resultado de aplicar Tm + CTM), resource_font_name, font_kind:
      Standard14 | EmbeddedSimple | EmbeddedComposite, text: String}`, `ImageItem {id, page,
      bbox, resource_xobject_name}`, `PageContent {text_runs: Vec<TextRun>, images:
      Vec<ImageItem>}`. Espejo de `annotation.rs`/`form.rs` en estilo, pero **no** se cuelga
      de `Page` — ver T-149. [ContentModel]
- [ ] T-149 API lazy: `pdf_edit::read_page_content(doc_bytes_or_ref, page: PageId) ->
      PageContent`, llamada por el shell al entrar en modo edición de esa página. `Document`
      no gana un campo `page_content` — decisión 2. [ContentModel]
- [ ] T-150 Variantes nuevas de `Command` (`#[non_exhaustive]`, cero breaking change) **con
      inversa completa desde el día uno** (mismo estándar que fijó B20 para forms, no el gap
      que dejó B5 con anotaciones): `ReplaceTextRunContent {page, item: TextRun, before:
      String, after: String}`, `InsertTextRun(TextRun)`, `RemoveTextRun(TextRun)`,
      `InsertImage(ImageItem)`, `RemoveImage(ImageItem)`, `MoveImage {page, item, from: Rect,
      to: Rect}`, `ResizeImage {page, item, from: Rect, to: Rect}`, `ReplaceImageSource
      {page, item, before: Vec<u8>, after: Vec<u8>}` + apply/inverse + tests undo/redo
      (patrón `edit_log`). [ContentModel, UndoRedo]

### Fase 2 — Crate `core/pdf-edit`
- [ ] T-151 Scaffold del crate (miembro del workspace; deps: pdf-document + lopdf), aislado
      del resto de core igual que pdf-sign/pdf-form. Módulos: `parse`, `encoding`, `edit`,
      `insert`. [infra]
- [ ] T-152 `parse.rs`: tokenizer + interpreter del content stream — trackea `q`/`Q`, `cm`
      compuesto y el text matrix (`Tm`/`Td`/`TD`) para computar el bbox de cada `TextRun`/
      `ImageItem` en espacio de página, correcto bajo rotación de página. Produce
      `PageContent`. [ContentParse]
- [ ] T-153 `encoding.rs`: mapea el texto de reemplazo/inserción a codes contra el
      `/Encoding` de la fuente del run (Standard-14/WinAnsi/MacRoman/Differences); devuelve
      `EncodingGap` tipado por carácter no representable en vez de escribir un glyph
      inválido. Fuentes Type0/CID: rechazadas siempre en v1 (decisión 3). [ContentEncoding]
- [ ] T-154 `edit.rs`: reescritura quirúrgica del content stream — `replace_text_run`
      reemplaza SOLO el operando de `Tj`/`TJ` del run targeteado, resto del stream byte a
      byte idéntico; `move_image`/`resize_image` reescriben el `cm` inmediatamente anterior
      al `Do` del XObject; `remove_item` elimina el rango de operadores del item; `replace_
      image_source` reemplaza el stream del XObject (`/Width`/`/Height`/`/Filter`
      recalculados) manteniendo el mismo nombre de recurso. [ContentEdit]
- [ ] T-155 `insert.rs`: agrega texto/imagen nuevos al final del content stream de la
      página (o a un stream nuevo si `/Contents` es array), registrando el recurso en
      `/Resources` — fuente Standard-14 vía el mismo helper `/DR` que pdf-form, o un XObject
      de imagen nuevo. [ContentInsert]

### Fase 3 — Serialización (`core/pdf-save`)
- [ ] T-156 `content.rs` sobre `ObjectSink`: rutea cualquier `Command` de contenido de
      página SIEMPRE por full rewrite en `strategy.rs` (decisión 5); si el documento tiene
      firmas existentes, expone la advertencia de invalidación al llamador — no bloquea el
      guardado, igual criterio que ya rige para otras ediciones estructurales. [ContentSave,
      FirmaCripto]
- [ ] T-157 Wiring: `bridge.rs` NO puebla contenido de página al abrir (decisión 2); expone
      el acceso lazy delegando directo a `pdf_edit::read_page_content` sobre el lopdf backing
      doc. [ContentSave]

### Fase 4 — FFI (`core/pdf-ffi`)
- [ ] T-158 `FfiTextRun`/`FfiImageItem`/`FfiPageContent` en types.rs; `FfiEditCommand` gana
      `ReplaceTextRunContent/InsertTextRun/RemoveTextRun/InsertImage/RemoveImage/MoveImage/
      ResizeImage/ReplaceImageSource`; `DocumentHandle::read_page_content(page) ->
      FfiPageContent` (carga lazy bajo demanda del shell). Smoke test: editar un run
      Standard-14 → save_to_bytes → reabrir → read_page_content devuelve el texto nuevo.
      [ContentFFI]

### Fase 5 — Fixtures e interop
- [ ] T-159 Fixtures: un PDF con texto en fuente Standard-14 (editable), uno con fuente
      embebida subset (para probar el camino `EncodingGap` → run no editable para cierto
      texto), uno con imagen (para move/resize/replace). Al menos uno generado por
      herramienta externa (mismo criterio que T-144 de B20). [ContentFixtures]
- [ ] T-160 Tests round-trip (editar → save full-rewrite → reabrir → el resto del contenido
      es byte-idéntico donde no fue tocado) + validador Python independiente (pypdf) que
      extrae texto del output y confirma que el run editado cambió. [Parity]

### Fase 6 — Docs
- [x] T-166 README: mover "Edit PDF" de `🔮 Planned` al roadmap activo; enlazar esta ficha
      (hecho en el mismo cambio que crea este documento).

## Tareas de UI (agregadas a la ficha de B8, docs/batches-b8-b13.md)

- [ ] T-161 (dep B21) Modo edición de contenido en el canvas: click sobre un text run
      existente abre un editor inline que preserva fuente/tamaño/posición; los runs no
      editables por `EncodingGap` se muestran distinguibles con explicación al usuario —
      nunca un fallo silencioso ni un intento que corrompa el stream. [ContentEdit]
- [ ] T-162 (dep B21) Imágenes de página real: seleccionar/mover/redimensionar con
      handles/reemplazar (file picker)/borrar imágenes existentes — distinto del stamp de
      anotación que ya existe (T-047). [ContentEdit]
- [ ] T-163 (dep B21) Insertar texto/imagen nuevos como contenido de página real; y el
      ciclo save→reopen→re-render obligatorio tras cualquier commit de edición de contenido
      (decisión 6) — acá el WARNING de UX diferido en T-046/T-047 deja de ser diferible.
      [ContentEdit]

## Criterios de aceptación

- Editar un text run en fuente Standard-14/simple con glyphs representables → el PDF
  resultante muestra el texto nuevo en Acrobat/Preview con la misma fuente/tamaño/posición.
- Un run cuya fuente no puede representar el texto de reemplazo (Type0/CID, o simple sin
  el glyph pedido) se detecta ANTES de escribir — nunca corrompe el content stream ni
  produce glyphs erróneos.
- Mover/redimensionar/reemplazar/borrar una imagen existente produce un PDF válido; el
  resto de la página es byte-idéntico donde no fue tocado.
- Insertar texto/imagen nuevos los agrega como contenido de página real (visible al
  extraer texto/imagen con herramientas externas), no como annotation.
- Cualquier commit de edición de contenido pasa por full rewrite; si invalida firmas
  previas, el shell lo advierte antes de guardar.
- El visor re-renderiza el resultado real tras cada commit (no un overlay simulado).
- Undo/redo completo desde el día uno para las ocho variantes de `Command` nuevas.

## Fuera de scope (v1)

Reflow de párrafos / wrap de texto (decisión de producto confirmada: sin motor de layout) ·
edición de texto en fuentes compuestas Type0/CID · cambiar fuente o tamaño de un run
existente (eso es "restyle", no "edit"; candidato a fase 2 igual que el flatten de
formularios) · OCR de contenido escaneado · redacción — estos dos últimos siguen post-MVP
per README.

## Orden de ejecución

Fase 1 → 2 → 3 → 4 lineal (cada fase compila y testea sola, TDD estricto — igual criterio
que B20). El fixture con fuente embebida subset (T-159) se necesita para cerrar T-153
(camino `EncodingGap`). La UI de B8 (T-161–163) arranca cuando T-158 (FFI) esté listo — B8
bypasea FFI para el resto del shell, pero igual consume el mismo modelo lazy de T-149/T-152
para no reimplementar el parser de content streams.
