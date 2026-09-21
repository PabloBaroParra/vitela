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
- [x] T-140 `FfiFormField`/`FfiTextStyle`/`FfiFieldValue`; `FfiEditCommand` gana
      `AddTextField/AddCheckbox/AddRadioGroup/AddDropdown/RemoveFormField/MoveFormField/
      ResizeFormField/RestyleFormField/SetFieldValue` (traducción en `build_core_command`
      resolviendo estados `from` actuales, patrón RemoveAnnotation);
      `DocumentHandle::list_form_fields() -> Vec<FfiFormField>` — **la API del panel
      lateral**; `next_form_field_id`; smoke test: crear campo → set value
      → save_to_bytes → reabrir → list_form_fields devuelve el campo con su valor. [FormFFI]
      **(2026-09-21 — completo. **No en `types.rs`**: los tipos y la traducción viven en un
      módulo nuevo, `core/pdf-ffi/src/form.rs`, por el mismo criterio con el que `compress.rs`
      es su propio módulo — lo que cruza es el vocabulario de un segundo crate (`pdf-form`,
      dependencia nueva de `pdf-ffi`) más la validación que ese vocabulario posee, no otra
      forma de editar una página. `types.rs` ya estaba en 706 líneas; solo recibe las
      variantes del enum de comandos, que no pueden vivir en otro lado.
      Siete tipos: `FfiFormField`, `FfiFormFieldKind`, `FfiFieldValue`, `FfiTextStyle`,
      `FfiFontFamily`, `FfiRadioOption`, `FfiFieldOrigin`.
      **Una variante de más y una decisión de menos.** `RenameFormField` no estaba en la
      lista de esta tarea y se agregó igual: el `Command` del core existe, `ops::rename_field`
      lo valida, y sin él este boundary sería el único lugar donde un campo se puede crear
      pero nunca nombrar — el inspector del shell GTK4 sí lo ofrece. En sentido contrario,
      los cuatro `Add*` **no** aceptan nombre: el `/T` lo elige `FormFieldSet::unique_name`
      con las mismas cuatro bases que usa el shell GTK4 (`Text`/`Checkbox`/`RadioGroup`/
      `Dropdown`), así que dos shells editando el mismo documento no producen dos esquemas
      de nombres. Quien quiera el suyo manda `RenameFormField` después, que es exactamente
      lo que hace el shell GTK4.
      **`next_form_field_id` no puede arrancar en 0 como `next_annotation_id`.** Las
      anotaciones nacen vacías en `document_from_lopdf`; los campos NO — `pdf_form::read_form_fields`
      los puebla al abrir, con ids que este boundary nunca emitió. Es uno más que el mayor
      en uso, leído del documento en cada comando en vez de ser un contador sembrado al abrir:
      undo/redo mueven campos dentro y fuera del set entre llamadas, y lo único que siempre
      sabe qué ids están gastados es el set. Test: guardar → reabrir → colocar un segundo
      campo, y los dos ids son distintos.
      **La compuerta de permisos es de dos bits, y ese es el hallazgo.** ISO 32000-1 tabla 22
      bit 6 cubre *rellenar* un campo existente por sí solo; crearlo o modificarlo exige
      además el bit 4 ("…and, if bit 4 is also set, create or modify interactive form
      fields"). Un documento que concede el 6 sin el 4 es real y legal. Así que
      `is_structural_form_command` separa los nueve estructurales de `SetFieldValue`, y
      `apply_edit` le pide el bit 4 solo a los primeros — el mismo corte que
      `forms::command::structural_edit_refusal` hace en el shell GTK4, para que un documento
      restringido se comporte igual en las dos plataformas. Ninguno queda excluido de
      `is_annotation_command`: el bit 6 es el piso de todos.
      **La trampa que eso esconde:** el `content_editing_is_allowed` *local* de `document.rs`
      no es el del core — le hace AND con `full_rewrite_blocker`. Un comando de formulario
      nunca fuerza rewrite (decisión 6 de T-139), así que usarlo habría rechazado la edición
      de campos en un AES-256 abierto con una sola contraseña, que guarda incremental sin
      problema. La compuerta llama a `pdf_manip::content_editing_is_allowed` directo. Por eso
      `form_field_editing_allowed()` existe como método propio en vez de dejar que el shell
      componga `annotation_editing_allowed() && content_editing_allowed()`, que daría la
      respuesta equivocada.
      **Validación antes de registrar, nunca en el save.** `set_field_value` y `rename_field`
      corren `pdf_form::ops` contra una *copia* del campo y recién ahí construyen el comando;
      el rename registra `validated.name`, no el string del caller, porque `ops` recorta
      espacios. `FormError` cruza como `UnsupportedOperation` (es un rechazo entendido, no una
      falla), y un id ausente como la variante nueva `FfiError::FormFieldNotFound`.
      `FfiFieldOrigin` descarta el id de objeto indirecto de `FieldOrigin::Existing` y conserva
      el único bit accionable: un campo que ya venía en el archivo lo rasteriza pdfium desde su
      propio `/AP`, así que un overlay que también dibuje su valor lo dibuja dos veces.
      Verificado en Windows: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
      -D warnings`, `cargo test --workspace` (86 binarios, 0 fallos). 12 tests de integración
      nuevos en `core/pdf-ffi/tests/forms.rs` — archivo propio, mismo criterio que
      `compress.rs` — incluido el criterio de aceptación de esta tarea end-to-end y sin mocks,
      más 14 unitarios en `form.rs` y 4 de compuerta en `document.rs` que construyen un
      `DocumentState` con permisos fabricados y prueban las tres combinaciones de bits.)**

### Fase 5 — Fixtures e interop
- [x] T-144 Fixture AcroForm generado en tests (patrón `labeled_pdf`) + **un fixture
      committeado creado por herramienta externa** (pypdf/reportlab, una sola vez,
      versionado en tests/fixtures/) para probar el parser contra AcroForm "de verdad". [FormRead]
      **(2026-09-21 — completo, y **encontró un bug real en la primera corrida**, que es
      exactamente para lo que esta tarea existe.
      El fixture committeado es `tests/fixtures/forms/reportlab_acroform.pdf` (~12 KB) con
      su generador al lado (`generate_reportlab_acroform.py`, no lo corre CI ni ningún
      test — mismo criterio "herramienta externa, generado una vez, versionado" que
      `content-edit/generate_reportlab_embedded_subset.py`, que ya citaba esta tarea como
      precedente). Trae un campo de cada tipo modelado + un **listbox**, que a propósito
      NO se modela: un fixture donde todo se lee no puede atrapar una regresión que empiece
      a modelar de más.
      **El bug:** reportlab repite el `/FT` heredado en cada widget kid de un radio group.
      `read::kids_are_widgets` exigía `!has(T) && !has(FT)` para considerar un kid como
      widget, así que leía esos kids como *campos propios* — un radio group se convertía en
      un checkbox anónimo por botón, los dos respondiendo al `/T` del padre. Dos campos con
      el mismo nombre, que es justo lo que `FormFieldSet` garantiza que no puede pasar.
      **Por qué nadie lo vio antes:** `/FT` es un atributo **heredable** (ISO 32000-1 tabla
      220) y un widget puede repetirlo legalmente; todos los fixtures de este repo están
      hechos con lopdf y ninguno lo escribe en los kids, así que la suite entera se daba la
      razón a sí misma. El discriminador correcto es `/T` y solo `/T`: un campo se direcciona
      por los `/T` de él y sus ancestros (§12.7.4.2), así que un kid sin `/T` no es
      direccionable y por lo tanto no es un campo. Fix de una línea, con dos tests nuevos que
      cubren las dos mitades de la regla.
      El fixture **generado** vive en código y no en disco:
      `gen_fixtures::forms::build_acroform_document` (una página, un text field vacío y un
      checkbox apagado, ambos merged field+widget), para los tests que necesitan dictar el
      estado inicial en vez de describir lo que reportlab haya emitido.
      7 tests nuevos en `core/pdf-form/tests/external_acroform.rs`.)**
- [x] T-145 Tests round-trip: crear los 4 tipos → save (incremental Y full) → reabrir →
      parsear → igualdad de modelo. Fill de PDF ajeno → save incremental → `/V` y `/AP`
      cambian, bytes originales intactos como prefijo. [FormSave, FirmaCripto]
      **(2026-09-21 — completo en `core/pdf-save/tests/forms_roundtrip.rs`, 6 tests + 2
      `#[ignore]` que exportan para T-146.
      Los dos writers se eligen por `original_bytes`: un documento autorado desde cero no
      tiene, así que va por full-rewrite; uno abierto sí, y como una edición de formulario
      nunca fuerza rewrite (decisión 6 de T-139) va por incremental. El test incremental
      afirma `saved.starts_with(&original_bytes)` además de la igualdad de modelo, así que
      la elección de writer queda clavada y no solo el resultado.
      El fill de PDF ajeno usa el fixture de reportlab de T-144. `/V` y `/AP` se verifican
      sobre el checkbox del fixture generado, donde los dos son visibles: `/V` es un `/Name`
      y `/AS` selecciona uno de los estados de `/AP /N`.
      **Igualdad de modelo, con tres exclusiones y una de ellas es un gap real.** `id` queda
      afuera porque `read_form_fields` reasigna ids secuencialmente en orden de `/Fields`
      (el orden sí se afirma, comparando las secuencias posición por posición) y `origin`
      porque todo campo releído es `Existing(oid)` por definición. La tercera es `style`, y
      no por ser impredecible — ver el ítem abierto al final de esta ficha.)**
- [x] T-146 Validador Python independiente con pypdf: abre el export de los tests, lee
      campos AcroForm y verifica nombre/tipo/valor (validador independiente de lopdf,
      mismo espíritu que la cross-validación de B12). [Parity, AnnoInterop]
      **(2026-09-21 — completo. `tools/pypdf-validation/validate_forms_roundtrip.py` con dos
      modos (`authored`, `filled`), su propio test (`test_validate_forms_roundtrip.py`, 14
      casos) y un job nuevo en `core.yml`: **AcroForm round-trip validation (T-146)**, con la
      misma estructura de los de T-160/T-174 — instalar pypdf con hashes, después producir y
      validar bajo `unshare --net`.
      Verifica `/FT`, `/V`, los bits de `/Ff` (multiline 13, radio 16, combo 18) y `/Opt`.
      Los flags importan tanto como el valor: son los que llevan el *tipo* al archivo, y un
      writer que los perdiera igual round-trippearía por nuestro propio reader, que tiene el
      modelo en la mano de los dos lados.
      El modo `filled` afirma tanto los dos campos que cambiaron como los **cuatro que no**,
      incluido el listbox que `pdf-form` nunca modela: "lo no modelado se preserva intacto"
      es un contrato sobre el archivo, y solo un lector que no seamos nosotros puede
      confirmarlo. pypdf lo ve intacto después de un save que reescribió el AcroForm.
      **Un paso extra, fuera de lo pedido y a propósito:** el job corre
      `python3 -m unittest discover -s tools/pypdf-validation` antes de validar nada. Los
      tests de los tres validadores (content T-160, metadata T-174 y este) no los corría
      **nadie** — un validador que no puede fallar no es evidencia, y uno que devolviera 0
      siempre habría pasado CI para siempre. Son 29 casos y un `discover` de una línea, más
      corto que apuntar solo al mío.)**

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

## Quién dibuja un campo: PDFium (detectado después de B20)

PDFium rasteriza el valor de cada campo de formulario que trae el archivo, a
partir del `/AP` del propio widget (`FPDF_ANNOT` y `do_render_form_data`, los
dos activos por defecto en `PdfRenderConfig`). Está verificado a nivel de
píxeles en `core/pdf-save/tests/preview_raster.rs`
(`pdfium_rasterizes_a_form_field_value_from_the_saved_file`), no deducido de
las fuentes de PDFium.

Como `pdf_form::read_form_fields` mete esos mismos valores en
`document.form_fields` al abrir, el shell Linux los dibujaba encima otra vez y
cada campo relleno de un formulario abierto salía doble.

**Decisión: la apariencia de un campo es de PDFium.** Es la única forma de
obtener el `/AP` real, la fuente real del `/DA` y el reparto real de un campo
comb o multilínea; la capa de dibujo solo puede aproximarlos. La regla está en
`selection::overlay_owns_field_value`.

- [x] La capa de dibujo pinta un valor solo donde PDFium no dibuja nada — es
      decir, donde el widget de los bytes abiertos está vacío o todavía no
      existe. `DocumentSession.rendered_field_values` es el registro de lo que
      traen esos bytes, tomado en `document::show_document` del modelo que
      acompaña al handle (igual que `backend_pages`) y deliberadamente **no**
      restaurado por `restore_edit_state`. Se indexa por nombre `/T` y no por
      `FormFieldId`: los ids se asignan secuencialmente en cada parseo.
- [x] `pdf_save::save_preview` escribe la capa de formularios. Solo omite el
      conjunto `annotations` del modelo, que sí es de la capa de dibujo.
- [x] Cada comando de campo mueve el ráster: `Command::is_form_field_edit`
      clasifica los siete, `forms::command::command` refresca al terminar cada
      gesto, `annotations::command::history` refresca al deshacer o rehacer, y
      `forms::fill::connect_settle` gasta uno cuando el foco sale del panel.
- [x] Rellenar un campo **vacío** sigue siendo inmediato en el canvas: ahí
      PDFium no dibuja nada, así que la capa de dibujo es dueña y pinta cada
      tecla. Es el caso normal de completar un formulario.

### Lo que cuesta, dicho de frente

- Sobrescribir un valor **que ya venía en el archivo** deja el valor viejo en
  el canvas hasta que el foco sale del panel de relleno. No es un solapamiento
  — nunca se ven los dos a la vez — es un retraso de una edición.
  `commit_value` graba un `SetFieldValue` por cada tecla, y un
  guardar→reabrir→re-renderizar por tecla no es pagable ni lo sobrevive el
  `Entry` en el que se está escribiendo (`toolbar::update_forms_controls`
  reconstruye `fill_rows`). Por eso se gasta uno solo, a la salida.
- Un `EventControllerFocus` sobre `fill_rows` informa de la entrada y salida
  del panel **como un todo**, no de los saltos entre sus propias filas: tabular
  de un campo al siguiente no reconstruye nada, que es la trampa que documenta
  T-143.
- Arrastrar el `SpinButton` del tamaño de fuente dispara un `RestyleFormField`
  por paso. `refresh_preview` los une (uno en vuelo más uno pendiente), así que
  una ráfaga se reduce a dos refrescos, no a uno por paso.

## Abierto: el estilo de un `/Btn` no sobrevive el round-trip (T-145)

Encontrado al escribir T-145, **no arreglado ahí**: cerrarlo es una decisión de
interop, no un test, y no pertenece al cambio que la destapó.

`forms::write_new_single_field` y `update_existing_single_field` escriben `/DA`
solo para `Tx` y `Ch`. El `TextStyle` de un checkbox o de un radio group no se
escribe en ningún lado, y al releer `parse_da` devuelve el default de la
decisión 3 (Helvetica 12pt negro).

Para `font` y `size_pt` da igual: nada los consume en un botón —
`appearance::build_field_appearance` dibuja el check en ZapfDingbats a un
tamaño derivado del rect. Para **`color` no da igual**: esa misma función sí lo
consume. Un tilde rojo se dibuja rojo, se hornea en `/AP`, y se relee negro. Y
el inspector del shell GTK4 ofrece fuente, tamaño y color para cualquier campo
seleccionado, sin distinción por tipo (`forms::style::refresh`) — así que el
usuario realmente puede poner un checkbox en rojo, realmente lo ve, y realmente
lo pierde al reabrir, mientras el archivo sigue pintando un color que el
inspector ya no muestra.

Clavado por `forms_roundtrip.rs::a_buttons_style_does_not_survive_the_round_trip`,
que **falla el día que esto se arregle** — ese es el punto.

Las dos salidas, con su costo:

1. **Escribir `/DA` para los cuatro tipos**, con el `format_da` que ya existe.
   Dos líneas. Round-trip exacto. Pero `/DA` es un atributo de *variable text*
   (tabla 222) y `/Btn` no lo es: un visor que regenere la apariencia desde
   `/DA` (con `NeedAppearances`, que nunca ponemos, pero una herramienta río
   abajo puede) buscaría `/Helv` donde espera `/ZaDb` y dibujaría el glifo
   equivocado.
2. **Escribir la forma convencional**, `/ZaDb 0 Tf <r g b> rg`, que es lo que
   escribe Acrobat. Interop correcto, pero hoy no alcanza: `try_parse_da`
   devuelve `None` salvo que encuentre **a la vez** un color y una de sus tres
   familias, así que un `/DA` con `/ZaDb` pierde también el color. Requiere que
   `parse_da` recupere el color independientemente de la fuente — cambio chico
   en `da.rs`, pero cambio al parser.

La 2 es la correcta si se arregla; la 1 es la barata. Ninguna es urgente: el
archivo se pinta bien, lo que miente es el inspector.

## Fuera de scope (v1)

Embedding de fuentes · listbox multi-select · JavaScript/acciones · validación de formato
(fechas, números) · XFA · flatten de formularios (candidato a fase 2) · UI (ficha B8,
T-141..T-143).

## Orden de ejecución

Fase 1 → 2 → 3 → 4 lineal (cada fase compila y testea sola, TDD estricto). El fixture
externo (T-144) se necesita al empezar T-137. Docs al final.
