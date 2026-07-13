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
- [ ] T-130 Módulo `form.rs`: `FormFieldId`, `FontFamily {Helvetica, TimesRoman, Courier}`,
      `TextStyle {font, size_pt, color}`, `FieldValue {Text, Checked, Choice(Option<String>)}`,
      `FormFieldKind {Text{multiline, max_len}, Checkbox, RadioGroup{options: Vec<RadioOption>},
      Dropdown{options, editable}}`, `FieldOrigin {New, Existing((u32, u16))}` (tupla cruda —
      pdf-document NO puede depender de lopdf), `FormField {id, page, name, rect, style,
      value, kind, origin}`, `FormFieldSet` Vec-backed (orden determinista) con
      `unique_name("Text") → Text_1, Text_2…`. Espejo de `annotation.rs`. [FormModel]
- [ ] T-131 `Document` gana `pub form_fields: FormFieldSet`; actualizar `Document::blank`,
      `document_from_lopdf` y tests existentes. [FormModel]
- [ ] T-132 Variantes nuevas de `Command` (es `#[non_exhaustive]`) **con inversa completa
      desde el día uno** (a diferencia del gap B5 de anotaciones): `AddFormField(FormField)`,
      `RemoveFormField(FormField)`, `MoveFormField{id, from: Rect, to: Rect}`,
      `ResizeFormField{id, from, to}`, `RestyleFormField{id, from: TextStyle, to: TextStyle}`,
      `SetFieldValue{id, from: FieldValue, to: FieldValue}` + apply/inverse + tests
      undo/redo (patrón edit_log). [FormModel, UndoRedo]

### Fase 2 — Crate `core/pdf-form`
- [ ] T-133 Scaffold del crate (miembro del workspace, dep: pdf-document + lopdf) con
      estructura espejo de pdf-annotate: `builders / ops / appearance / error` + `read` + `da`.
      Builders puros: `text_field`, `checkbox`, `radio_group`, `dropdown`. [FormBuilders]
- [ ] T-134 `ops.rs`: `move_field`, `resize_field`, `restyle_field`, `set_value` con
      validación (`Checked` solo en checkbox; `Choice` ∈ options si `!editable`; `max_len`). [FormOps]
- [ ] T-135 `da.rs`: serializar/parsear `/DA` (`"0 0 0 rg /Helv 12 Tf"` ↔ `TextStyle`),
      defaults Helv 12 negro al fallar parseo. [FormStyle]
- [ ] T-136 `appearance.rs`: streams `/AP /N` por tipo — texto con clipping al rect y
      word-wrap greedy para multilínea; checkbox estados `/N << /Yes /Off >>` con check
      ZapfDingbats; radio un kid-widget por opción con sus estados; dropdown muestra el
      valor seleccionado. Ids indirectos como `Reference((0,0))` placeholder — la numeración
      real la pone pdf-save (mismo contrato que pdf-annotate::appearance). [FormAppearance]
- [ ] T-137 `read.rs` — parser de AcroForms existentes: camina `/AcroForm /Fields`, resuelve
      field tree (padres/kids, nombres fully-qualified `padre.hijo`), mapea widgets a páginas
      por `/Annots`, extrae `/FT`, `/V`, `/Ff` (bit 13 multiline, 16 radio, 18 combo,
      19 edit), `/Opt`, `/MK`, `/DA`, `/Rect` → `Vec<FormField>` con `origin: Existing(oid)`.
      Lo no modelado se preserva intacto y queda fuera del set editable. Reusar el patrón
      `resolve_object` de pdf-save/bridge.rs:239. [FormRead, AnnoInterop]

### Fase 3 — Serialización (`core/pdf-save`)
- [ ] T-138 `forms.rs` sobre `ObjectSink` (annotations.rs:31): `ensure_acroform(sink,
      catalog_id) -> ObjectId` crea/obtiene `/AcroForm` con `/Fields` y `/DR` standard-14.
      **PÚBLICO y documentado — lo reusa el wiring del save layer que inserta el campo
      `/Sig` de T-073 (el `SignatureFieldBuilder` de pdf-sign, ya implementado, construye
      los diccionarios pero delega `/AcroForm /Fields` y `/Annots` al guardado).**
      `write_form_fields`: nuevos → field+widget fusionado, append a `/Fields` + `/Annots`
      (conviviendo con `page_annotation_objects`); existentes modificados → clone-and-modify
      (rect//DA//V) + regenerar `/AP`. Radio: field padre con `/Kids`. [FormSave]
- [ ] T-139 Wiring: `strategy.rs` trata cambios de formulario como no estructurales
      (vía incremental y full-rewrite); `bridge.rs::document_from_lopdf` puebla
      `form_fields` vía `pdf_form::read` (population-on-open). Determinismo: orden de
      escritura = orden del FormFieldSet (check byte-idéntico de CI). [FormSave, Parity]

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
