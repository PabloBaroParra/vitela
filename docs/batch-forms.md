# Ficha de batch B20 — Formularios rellenables (AcroForm)

> Plan aprobado 2026-07-13 (plan file: `~/.claude/plans/quiero-una-cosa-para-snug-hoare.md`).
> Cambio de scope explícito: el README declaraba forms fuera del MVP; este batch lo trae
> adentro. Alcance confirmado: set completo de tipos, interop de lectura con AcroForms
> ajenos, core ahora + UI diferida a B8 (ver tareas T-141..T-143 agregadas a esa ficha).

**Dependencias:** B6 ✓ (crates core). Independiente de los shells — paralelo a B8/B9/B10/B13.
La numeración B20 deja libre B14 (iOS) y B15–B19 (reserva de firma criptográfica).

## Hecho clave del formato

Un campo de formulario es un **field dictionary** (en `/AcroForm /Fields` del catálogo) cuya
representación visual es una anotación `/Subtype /Widget` en el `/Annots` de la página
(usualmente field+widget fusionados en un dict). Toda la infraestructura de anotaciones
(ObjectSink, preservación de `/Annots` ajenos, EditLog) aplica casi 1:1.

## Decisiones de diseño (fijadas en el plan aprobado)

1. **Crate nuevo `core/pdf-form`** (no extender pdf-annotate): los widgets traen estado
   documento-level (`/AcroForm`, `/DR`, nombres `/T` únicos, field tree) ajeno a markup
   annotations. Mismo criterio de aislamiento que pdf-sign.
2. **Generar `/AP` siempre** — no confiar en `/NeedAppearances` (Preview lo ignora).
3. **Fuentes Standard 14 únicamente** (Helvetica/Times/Courier vía `/DR`, Type1 no embebido).
   Estilo por campo = fuente + tamaño + color RGB, serializado en `/DA`.
4. **Relleno en vivo = overlay del shell**, no re-render pdfium. `/V` + `/AP` regenerado se
   escriben al guardar (esquiva la limitación "render refleja solo lo guardado" de pdf-ffi).
5. **`origin: New | Existing(ObjectId)`** en el modelo → save incremental hace
   clone-and-modify del dict original sin duplicar campos ajenos.
6. Cambios de formulario son **no estructurales** → vía incremental válida (no invalida
   firmas — coherente con la regla de oro de B12). `/FT /Sig` y tipos no soportados
   (pushbutton, listbox multi-select, JS actions, XFA) quedan opaco-preservados.

## Tareas

### Fase 1 — Modelo (`core/pdf-document`)
- [x] T-130 Módulo `form.rs`: `FormFieldId`, `FontFamily {Helvetica, TimesRoman, Courier}`,
      `TextStyle {font, size_pt, color}`, `FieldValue {Text, Checked, Choice(Option<String>)}`,
      `FormFieldKind {Text{multiline, max_len}, Checkbox, RadioGroup{options: Vec<RadioOption>},
      Dropdown{options, editable}}`, `FieldOrigin {New, Existing((u32, u16))}` (tupla cruda —
      pdf-document NO puede depender de lopdf), `FormField {id, page, name, rect, style,
      value, kind, origin}`, `FormFieldSet` Vec-backed (orden determinista) con
      `unique_name("Text") → Text_1, Text_2…`. Espejo de `annotation.rs`. [FormModel]
      **(2026-08-30 — completo. `RadioOption {export_value, rect}`: cada botón de un
      `RadioGroup` es un kid-widget con su propia posición en la página — el `rect` del
      `FormField` padre queda como bbox del campo en su conjunto; `ops.rs` (T-134) resuelve
      cómo mover/redimensionar el grupo moviendo sus opciones. `FormFieldSet` suma
      `get_mut`, que `AnnotationSet` no necesita, porque los comandos de campo (T-132)
      mutan un solo atributo in-place en vez de reemplazar todo el valor.)**
- [x] T-131 `Document` gana `pub form_fields: FormFieldSet`; actualizar `Document::blank`,
      `document_from_lopdf` y tests existentes. [FormModel]
      **(2026-08-30 — completo. `Document::blank` no necesitó cambios propios —
      `#[derive(Default)]` ya cubre el campo nuevo. El único struct-literal de `Document`
      fuera de este crate es `bridge.rs::document_from_lopdf`; population real vía
      `pdf_form::read` queda para T-139, acá es `Default::default()` — un documento
      abierto hoy simplemente no ve sus AcroForms existentes todavía.)**
- [x] T-132 Variantes nuevas de `Command` (es `#[non_exhaustive]`) **con inversa completa
      desde el día uno** (a diferencia del gap B5 de anotaciones): `AddFormField(FormField)`,
      `RemoveFormField(FormField)`, `MoveFormField{id, from: Rect, to: Rect}`,
      `ResizeFormField{id, from, to}`, `RestyleFormField{id, from: TextStyle, to: TextStyle}`,
      `SetFieldValue{id, from: FieldValue, to: FieldValue}` + apply/inverse + tests
      undo/redo (patrón edit_log). [FormModel, UndoRedo]
      **(2026-08-30 — completo. `apply` resuelve Move/Resize/Restyle/SetFieldValue vía
      `FormFieldSet::get_mut` (mutación in-place de un solo campo del `FormField`), a
      diferencia de `ReplaceAnnotation` que reemplaza el valor entero — la forma la fija el
      propio comando (`from`/`to` de un solo atributo), no una preferencia de estilo.
      `is_content_edit()` no lista ninguna variante nueva: quedan `false` por default, igual
      que las de anotaciones/página, porque `Document` sí las modela (a diferencia de las
      nueve de Batch 21, que son inertes). Gate en Windows (cross-platform, sin gate de
      `target_os`): `cargo fmt --check` + `clippy --all-targets -D warnings` + `cargo test`
      en `pdf-document` y `pdf-save`, todo verde (60 tests en pdf-document, sin regresiones
      en pdf-save).)**

### Fase 2 — Crate `core/pdf-form`
- [x] T-133 Scaffold del crate (miembro del workspace, dep: pdf-document + lopdf) con
      estructura espejo de pdf-annotate: `builders / ops / appearance / error` + `read` + `da`.
      Builders puros: `text_field`, `checkbox`, `radio_group`, `dropdown`. [FormBuilders]
      **(2026-08-30 — completo: `core/pdf-form` scaffoldeado con `builders`/`ops`/`error`/`da`
      (los cuatro que ya tienen contenido); `appearance`/`read` quedan para T-136/T-137 —
      declarar el módulo antes de que exista contenido real habría sido puro ruido. Los
      cuatro builders son infalibles (a diferencia de `stamp_from_image_bytes`, que decodifica
      bytes externos): un campo recién construido siempre arranca sin valor, la validación
      contra el kind es responsabilidad de `ops::set_value`, no de la construcción.)**
- [x] T-134 `ops.rs`: `move_field`, `resize_field`, `restyle_field`, `set_value` con
      validación (`Checked` solo en checkbox; `Choice` ∈ options si `!editable`; `max_len`). [FormOps]
      **(2026-08-30 — completo. `move_field`/`resize_field`/`restyle_field` son infalibles —
      a diferencia de `pdf-annotate::ops` (donde un `Ink` no tiene `rect` y una `Stamp` no
      tiene color), `rect` y `style` son campos incondicionales de `FormField` para
      cualquier `FormFieldKind`, así que no hay variante que rechazar. Solo `set_value`
      valida, y la regla de "options si !editable" quedó dividida por kind como el spec la
      redacta: `RadioGroup` siempre exige que la opción exista (no tiene concepto de
      "editable" — eso es exclusivo de un combo box), `Dropdown` solo lo exige cuando
      `editable: false`. Limpiar la selección (`Choice(None)`) siempre se acepta en ambos,
      sea cual sea `editable`.)**
- [x] T-135 `da.rs`: serializar/parsear `/DA` (`"0 0 0 rg /Helv 12 Tf"` ↔ `TextStyle`),
      defaults Helv 12 negro al fallar parseo. [FormStyle]
      **(2026-08-30 — completo. Nombres de recurso `/DR /Font` fijados a la convención de
      Acrobat (`Helv`/`TiRo`/`Cour`) porque el propio ejemplo de la ficha los da por sentado
      y T-138 los va a reusar para poblar `/DR`. `parse_da` es deliberadamente infalible —
      nunca `Result` — porque un `/DA` ajeno puede traer sintaxis que este crate no modela
      (CMYK `k`, operadores extra como `Tr`) y T-137 no puede dejar que un campo mal formado
      aborte la lectura de todo el AcroForm; ante cualquier gap (operador de color o `Tf`
      ausente, nombre de fuente no reconocido) el resultado es el default completo de la
      decisión 3 (Helvetica/12pt/negro), nunca un merge parcial. El parser tolera color y
      fuente en cualquier orden con un stack de operandos que cada operador consume y
      limpia — probado con ambos órdenes y con `g` (gris) además de `rg`.)**
- [x] T-136 `appearance.rs`: streams `/AP /N` por tipo — texto con clipping al rect y
      word-wrap greedy para multilínea; checkbox estados `/N << /Yes /Off >>` con check
      ZapfDingbats; radio un kid-widget por opción con sus estados; dropdown muestra el
      valor seleccionado. Ids indirectos como `Reference((0,0))` placeholder — la numeración
      real la pone pdf-save (mismo contrato que pdf-annotate::appearance). [FormAppearance]
      **(2026-08-30 — completo, con un ajuste al contrato de "Reference((0,0))": ninguno de
      los streams de este batch referencia OTRO objeto (a diferencia del `/SMask` de
      pdf-annotate), así que no hace falta placeholder — `pdf-save` (T-138) solo necesita
      numerar cada `Stream` y armar el diccionario `/AP` alrededor, sin reescribir nada
      dentro de ellos. Word-wrap reusa `pdf_edit::encoding::tables::standard_14_ascii_widths`
      en vez de copiar una segunda tabla AFM — nueva dependencia `pdf-form → pdf-edit`
      acotada a esa función pública. `parse`/`build_field_appearance` rechaza cualquier
      carácter fuera de ASCII imprimible (32-126) con `FormError::InvalidValue` — es el
      análogo de formularios al `EncodingGap` de edición de contenido: decisión 3 no
      embebe fuente, así que no hay glifo de repuesto para nada fuera de esa tabla; nunca
      se corrompe el stream con un carácter no representable. Glifos ZapfDingbats fijados
      a la convención de Adobe/reportlab (checkmark = código `0x34`, círculo relleno =
      `0x6C`) vía el recurso `/ZaDb`, independiente de la fuente elegida por el usuario en
      `TextStyle` (el color sí se respeta). El estado "Off" de checkbox/radio es un stream
      vacío a propósito — `/MK` (borde/fondo) queda fuera de scope v1, así que un control
      sin marcar no pinta nada. Multilínea envuelve por párrafo (separando por `\n` del
      valor) antes de aplicar wrap greedy dentro de cada uno, para que un salto de línea
      que el usuario tipeó se respete y no solo el desborde de ancho.)**
- [x] T-137 `read.rs` — parser de AcroForms existentes: camina `/AcroForm /Fields`, resuelve
      field tree (padres/kids, nombres fully-qualified `padre.hijo`), mapea widgets a páginas
      por `/Annots`, extrae `/FT`, `/V`, `/Ff` (bit 13 multiline, 16 radio, 18 combo,
      19 edit), `/Opt`, `/MK`, `/DA`, `/Rect` → `Vec<FormField>` con `origin: Existing(oid)`.
      Lo no modelado se preserva intacto y queda fuera del set editable. Reusar el patrón
      `resolve_object` de pdf-save/bridge.rs:239. [FormRead, AnnoInterop]
      **(2026-08-30 — completo, con resolución de referencias de un solo salto (mirror de
      `pdf_edit::encoding::resolve`, no la cadena acotada a 32 de `bridge.rs`) — una
      referencia que ese único salto no resuelve simplemente hace fallar la `Option` que la
      envuelve, y acá eso significa "saltear este campo", nunca abortar la lectura entera.
      `/MK` no se lee (no hay nada del modelo que lo use — decisión explícita, ver Fuera de
      Scope) pese a estar listado en la tarea original.
      **La distinción campo-terminal vs. nodo-de-agrupación** (necesaria porque ambos usan
      `/Kids`) se resuelve mirando los KIDS, no el nodo: si todos los kids carecen de `/T`
      Y de `/FT` propios son widgets del MISMO campo (el caso RadioGroup); si algún kid
      tiene su propio `/T` o `/FT` son campos hijos genuinos y se recursa extendiendo el
      nombre calificado, sin modelar el nodo agrupador en sí.
      **RadioGroup no tiene un `/Rect` propio** (cada botón trae el suyo) — `FormField.rect`
      queda como el bounding box de las opciones, y si CUALQUIER kid no resuelve `/Rect` o
      `/AP /N` limpio se descarta el grupo COMPLETO en vez de modelarlo a medias (un radio
      con un botón faltante es peor que uno no modelado).
      **El checkbox no preserva el nombre real del estado "on"** (p.ej. si el archivo usaba
      `/On` en vez de `/Yes`): solo se lee si `/V` es distinto de `/Off`/ausente para
      `Checked(bool)`. Es una decisión, no un gap — T-138 siempre reescribe `/V` y `/AP/N`
      con la convención fija de este crate (`Yes`/`Off`) de forma consistente entre sí, así
      que el checkbox re-guardado sigue siendo un checkbox funcional aunque el nombre
      interno cambie; ningún consumidor real (Acrobat incluido) depende de ese string.
      **Dropdown solo modela el valor de display de `/Opt`** (no el par export/display por
      separado) — coherente con `FormFieldKind::Dropdown { options: Vec<String> }` ya fijado
      en T-130; si una entrada de `/Opt` es un array `[export, display]` se toma el display.
      Verificado con fixtures propias (patrón `labeled_pdf` de `pdf-save::bridge`, ver
      `FormFixture` en los tests del módulo): texto simple, multilínea+`/MaxLen`, checkbox
      marcado/sin marcar, radio group con selección, dropdown con opciones de display,
      choice field sin bit Combo (listbox, sin modelar), pushbutton (sin modelar), `/FT /Sig`
      (sin modelar), grupos de nombre anidados (`address.street`), asignación secuencial de
      `FormFieldId`, y un campo sin anotación en ninguna página (sin modelar). 49/49 tests de
      `pdf-form`, `cargo fmt --check` + `clippy --all-targets -D warnings` limpios.
      **Pendiente real, no cerrado acá:** el fixture COMMITTEADO generado por una herramienta
      externa (pypdf/reportlab) que pide T-144 para probar contra un AcroForm "de verdad"
      no está hecho — esta sesión no verificó disponibilidad de un toolchain Python. Queda
      abierto en T-144.)**

### Fase 3 — Serialización (`core/pdf-save`)
- [x] T-138 `forms.rs` sobre `ObjectSink` (annotations.rs:31): `ensure_acroform(sink,
      catalog_id) -> ObjectId` crea/obtiene `/AcroForm` con `/Fields` y `/DR` standard-14.
      **PÚBLICO y documentado — lo reusa el wiring del save layer que inserta el campo
      `/Sig` de T-073 (el `SignatureFieldBuilder` de pdf-sign, ya implementado, construye
      los diccionarios pero delega `/AcroForm /Fields` y `/Annots` al guardado).**
      `write_form_fields`: nuevos → field+widget fusionado, append a `/Fields` + `/Annots`
      (conviviendo con `page_annotation_objects`); existentes modificados → clone-and-modify
      (rect//DA//V) + regenerar `/AP`. Radio: field padre con `/Kids`. [FormSave]
      **(2026-08-30 — completo. `page_dict_mut` de `ObjectSink` NO es en realidad
      page-specific en ninguna de las dos implementaciones (Document/IncrementalDocument):
      es un "traeme este dict, clonándolo a la revisión nueva primero si el writer es
      incremental" genérico. Se reusa tal cual para el dict de `/AcroForm`, el catálogo, y
      el dict de un field/kid EXISTENTE — cero trait nuevo.
      **Por qué actualizar un campo existente nunca necesita leer la base por separado:**
      en el full-rewrite `working` ya arranca como clon completo de la base
      (`replay_page_ops`), y en el incremental `page_dict_mut` clona el objeto de la
      revisión previa a la nueva revisión en el primer touch. En ambos casos
      `sink.page_dict_mut(existing_oid)` YA devuelve el contenido actual del campo —a
      diferencia de `page_annotation_objects`, que sí necesita leer `input.base` aparte
      porque las anotaciones son solo-aditivas en el camino incremental.
      **Nuevo vs. existente determina qué se apendea:** un campo NUEVO necesita su id en
      `/Annots` de su página Y en `/AcroForm /Fields` (nada lo referenciaba antes); uno
      EXISTENTE ya está referenciado por ambos desde el archivo original y mantiene el
      mismo id (decisión 5: clone-and-modify), así que actualizarlo no toca ninguno de los
      dos arrays. El padre de un RadioGroup es la única excepción real: al no tener widget
      propio nunca entra a ningún `/Annots`, solo a `/Fields`.
      **Bug real encontrado por los tests, no por inspección:** `Dictionary::set(key,
      alguna_String)` en esta versión de lopdf produce `Object::Name`, NO
      `Object::String` — el mismo patrón que anotaciones ya usa correctamente
      (`Object::string_literal(...)` explícito para `/Contents`) pero que esta ficha pasó
      por alto al escribir `/T` y `/DA` la primera vez. Sin el fix, `/T` (debe ser PDF
      string) y `/DA` (debe ser PDF string) se escribían como `/Name` — inválido per spec,
      y habría roto el round-trip de estilo silenciosamente (`da_of`/`parse_da` de T-137
      esperan `.as_str()`, fallarían y caerían al default). Dos tests que comparaban con
      `.as_str()` sobre un valor que SÍ debía ser `/Name` (`/AS`) fallaron primero y
      apuntaron directo al problema real.
      **La invariante de correspondencia posicional para radio groups existentes:**
      `FieldOrigin::Existing(oid)` solo guarda el id del field PADRE, no los ids de los
      kids — `update_existing_radio_group` los reobtiene leyendo `/Kids` del propio dict
      (ya trae el contenido actual, ver arriba) y los empareja posicionalmente con
      `options[i]`. Sostenido porque `pdf_form::read` construye `options` caminando
      `/Kids` en orden de array, y ningún `Command` de este crate reordena o le cambia el
      tamaño a esa lista — se documenta como invariante de todo el crate, no un detalle
      de implementación.
      Verificado: `cargo fmt --check` + `clippy --all-targets -D warnings` + `cargo test`
      en Windows, 71 tests nuevos en `pdf-save` (`forms::tests` + el smoke end-to-end de
      abajo), sin regresiones en las suites existentes de anotaciones/contenido/estrategia.)**
- [x] T-139 Wiring: `strategy.rs` trata cambios de formulario como no estructurales
      (vía incremental y full-rewrite); `bridge.rs::document_from_lopdf` puebla
      `form_fields` vía `pdf_form::read` (population-on-open). Determinismo: orden de
      escritura = orden del FormFieldSet (check byte-idéntico de CI). [FormSave, Parity]
      **(2026-08-30 — completo: `write_form_fields` se llama en `save_full_rewrite` (tras
      `attach_annotations`, antes de `set_mod_date`) y dentro del closure de
      `save_incremental` (tras `attach_annotations`), ambas resolviendo `/Root` una sola
      vez vía el nuevo `catalog_object_id`. `document_from_lopdf` puebla `form_fields`
      recorriendo `pdf_form::read_form_fields(lopdf.as_lopdf())` — el orden de escritura
      hereda el determinismo que T-137 ya documentó para la lectura (orden de `/Fields`).
      Ningún cambio a `requires_full_rewrite`: decisión 6 confirma que los comandos de
      formulario nunca fuerzan rewrite por sí mismos.
      **Test end-to-end nuevo** (`strategy::tests::
      save_document_writes_a_new_form_field_that_reads_back`) prueba el pipeline completo
      sin mocks: `Command::AddFormField` → `save_document` → `lopdf::Document::load_mem` →
      `pdf_form::read_form_fields` — el campo vuelve con su nombre y valor intactos, contra
      la implementación REAL de ambos lados (T-137 leyendo lo que T-138 escribió), no solo
      contra los fixtures unitarios de cada módulo por separado. Pasó a la primera.)**

### Fase 4 — FFI (`core/pdf-ffi`)
- [ ] T-140 `FfiFormField`/`FfiTextStyle`/`FfiFieldValue` en types.rs; `FfiEditCommand` gana
      `AddTextField/AddCheckbox/AddRadioGroup/AddDropdown/RemoveFormField/MoveFormField/
      ResizeFormField/RestyleFormField/SetFieldValue` (traducción en `build_core_command`
      resolviendo estados `from` actuales, patrón RemoveAnnotation);
      `DocumentHandle::list_form_fields() -> Vec<FfiFormField>` — **la API del panel
      lateral**; `next_form_field_id` en DocumentState; smoke test: crear campo → set value
      → save_to_bytes → reabrir → list_form_fields devuelve el campo con su valor. [FormFFI]

### Fase 5 — Fixtures e interop
- [ ] T-144 Fixture AcroForm generado en tests (patrón `labeled_pdf`) + **un fixture
      committeado creado por herramienta externa** (pypdf/reportlab, una sola vez,
      versionado en tests/fixtures/) para probar el parser contra AcroForm "de verdad". [FormRead]
- [ ] T-145 Tests round-trip: crear los 4 tipos → save (incremental Y full) → reabrir →
      parsear → igualdad de modelo. Fill de PDF ajeno → save incremental → `/V` y `/AP`
      cambian, bytes originales intactos como prefijo. [FormSave, FirmaCripto]
- [ ] T-146 Validador Python independiente con pypdf: abre el export de los tests, lee
      campos AcroForm y verifica nombre/tipo/valor (validador independiente de lopdf,
      mismo espíritu que la cross-validación de B12). [Parity, AnnoInterop]

### Fase 6 — Docs
- [x] T-147 README: mover forms de "out of scope" al roadmap/features; enlazar esta ficha. [docs]

## Criterios de aceptación

- Export abre en Acrobat/Preview/Firefox/Chrome y los campos son rellenables ahí (los 4
  tipos: texto una línea/multilínea, checkbox, radio group, dropdown).
- PDF ajeno con AcroForm → sus campos aparecen en `list_form_fields` con valores actuales;
  editarlos y guardar produce un PDF que Acrobat sigue reconociendo como el mismo formulario.
- Fill + save incremental NO invalida firmas existentes (bytes originales = prefijo intacto).
- Campos `/FT /Sig`, pushbuttons, listbox multi-select y acciones JS se preservan intactos
  sin aparecer como editables. XFA: no soportado jamás.
- Estilo (fuente standard-14, tamaño, color) round-tripea por `/DA`.
- Undo/redo completo para add/remove/move/resize/restyle/set_value desde el día uno.
- Salida determinista bajo el clock/ID-generator de CI (orden = FormFieldSet).

## Fuera de scope (v1)

Embedding de fuentes · listbox multi-select · JavaScript/acciones · validación de formato
(fechas, números) · XFA · flatten de formularios (candidato a fase 2) · UI (ficha B8,
T-141..T-143).

## Orden de ejecución

Fase 1 → 2 → 3 → 4 lineal (cada fase compila y testea sola, TDD estricto). El fixture
externo (T-144) se necesita al empezar T-137. Docs al final.
