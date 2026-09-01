# Ficha de batch B22 — Metadatos del documento (Info Dictionary)

> Documentado 2026-08-13, a pedido explícito del usuario, para encolarlo — **no arranca
> ahora**. Intención de secuencia: se ejecuta último entre los batches de núcleo
> (B20 → B21 → B22), justo antes de que el foco pase de lleno a la UI de los shells
> (B8/B9/B10/B12/B13).
>
> Scope acotado en la conversación que originó este documento: el pedido inicial
> mostraba la pestaña "Detalles" de Propiedades de archivo de Windows Explorer (Nombre,
> Tipo, Ubicación, Tamaño, Fecha de creación/modificación, Atributos, Compartido con,
> Propietario, Equipo) y pedía poder editar "todos esos metadatos, el propietario y
> demás" en todas las plataformas. **Eso no es metadata del PDF — es metadata del
> sistema de archivos.** Propietario/Compartido con/Equipo son ACL y compartición de
> red de Windows sin equivalente significativo fuera de NTFS; en iOS/Android la app ni
> siquiera tiene acceso a ese nivel de filesystem (storage sandboxeado sin ACL/owner).
> Meterlo en scope convertiría esto en código nativo distinto por SO, ajeno al core
> Rust compartido, y sin sentido en dos de las cinco plataformas objetivo. Decisión
> confirmada por el usuario: **este batch cubre únicamente el Info Dictionary del PDF**
> (`/Title`, `/Author`, `/Subject`, `/Keywords`, `/Creator`, `/Producer`,
> `/CreationDate`, `/ModDate`) — datos que viven dentro de los bytes del archivo y son
> genuinamente cross-platform. Propiedades de SO quedan fuera de scope (ver "Fuera de
> scope").

**Dependencias:** B6 ✓ (crates core). Independiente de los shells — mismo patrón que
B20/B21 (paralelo a B8/B9/B10/B13), pero por decisión explícita de secuencia se hace
**último** entre los batches de núcleo. Numeración: siguiente libre tras B21.

## Hecho clave del formato

El `/Info` del trailer es una referencia a un diccionario simple — a diferencia de
anotaciones, campos de formulario o contenido de página, no hay parser propio que
escribir: son pares clave/valor donde el valor es un string PDF o, para las fechas, un
string con formato fijo `D:YYYYMMDDHHmmSSOHH'mm'` (PDF 32000-1:2008 §7.9.4). El spec
define ocho claves estándar (las siete de texto más `/Trapped`, no cubierta acá) y
permite claves custom arbitrarias, que este batch no expone (ver "Fuera de scope").

El proyecto ya toca `/Info` hoy, aunque no lo expone como feature: `pdf-save`
(`strategy.rs::set_mod_date`, `clock.rs`) **pisa `/ModDate` en cada guardado con full
rewrite**, vía un `Clock` inyectable (`SystemClock` en producción, `FixedClock` en
tests, para guardados byte-idénticos en CI). Para documentos sin `/Info` previo,
también fija `/CreationDate` al mismo instante. El guardado incremental
(`save_incremental`) **deja `/Info` intacto a propósito** — comentario explícito en
`strategy.rs:256-264` documentando el diferido: el `new_document` de un incremental
nace como clon del trailer previo, así que `/Info` ya apunta a un objeto de una
revisión anterior sin tocar; clonarlo antes de mutar quedó pendiente. Este batch tiene
que convivir con ambos hechos, no reinventarlos.

## Decisiones de diseño

1. **Sin crate nuevo.** A diferencia de pdf-sign/pdf-form/pdf-edit, no hay parser ni
   dependencias pesadas que aislar — vive en `core/pdf-document` (`metadata.rs`) y
   `core/pdf-save`, igual que hoy vive `set_mod_date`.
2. **Alcance de campos: los siete estándar de texto** (`Title`, `Author`, `Subject`,
   `Keywords`, `Creator`, `Producer`, más las dos fechas `CreationDate`/`ModDate`).
   `Trapped` y claves custom quedan fuera de v1 — mismo criterio conservador que B21
   con fuentes Type0/CID: cubrir bien lo común antes que mal lo raro.
3. **`DocumentInfo` con campos `Option<...>`: `None` = clave ausente del `/Info`
   dict**, no "clave con string vacío". Editar un campo a vacío en la UI borra la
   clave al guardar, no escribe `()`. Evita diffs ruidosos y dicts con claves basura.
4. **`PdfDate` como value type**, no `String` crudo. Hoy `clock.rs` solo *formatea*
   (`pdf_date_string_from_unix_secs`) — nunca parsea, porque nunca tuvo que leer una
   fecha existente. Un campo editable de "fecha de creación" en UI necesita mostrar el
   valor actual, así que el parser es trabajo nuevo, no reutilización. Round-trip
   parse→format fijado por test.
5. **`Command::SetDocumentInfo { before, after }`, no un comando por campo.** El
   `/Info` dict es chico y se edita como unidad desde un panel único — partirlo en ocho
   comandos granulares (como si fueran `TextRun`s independientes de B21) sería
   estructura sin beneficio: nadie deshace "solo el autor" sin also querer ver el resto
   del formulario. Inversa trivial (swap before/after), mismo estándar de "inversa
   completa desde el día uno" de B20/B21.
6. **Precedencia de `ModDate` explícito sobre el auto-stamp.** Si `after.mod_date` es
   `Some(_)`, ese valor gana en ESE guardado y `set_mod_date` no lo pisa. Cualquier
   guardado posterior que no repita el comando vuelve al auto-stamp de siempre — el
   comportamiento actual (pisar `/ModDate` en cada save) no cambia para el caso común,
   solo se vuelve overridable cuando el usuario edita esa fecha a propósito.
7. **Codificación de texto: PDFDocEncoding cuando alcanza, UTF-16BE con BOM (`FE FF`)
   cuando no** (§7.9.2.2). A diferencia del `EncodingGap` de B21 (donde un run puede
   quedar sin poder editarse), acá **no hay caso sin salida**: UTF-16BE cubre todo
   Unicode, así que cualquier texto que el usuario tipee es representable — la decisión
   es solo qué encoding usar, nunca rechazar la edición.
8. **El incremental gana soporte de metadatos, no lo evita con full rewrite.** A
   diferencia de B21 (que rutea todo por full rewrite porque el contenido de página
   siempre es cambio estructural), un cambio de metadatos es chico y frecuente — forzar
   full rewrite sería el mismo tipo de costo que B20 evitó a propósito para el relleno
   de formularios. En cambio, se cierra el diferido de `strategy.rs:256-264`: clonar
   `/Info` en `new_document` antes de mutar (mismo patrón que `page_dict_mut`), pero
   **solo cuando hay un `SetDocumentInfo` pendiente** — sin comando de metadatos, el
   incremental sigue sin tocar `/Info`, cero cambio de comportamiento.
9. **Ningún auto-stamp nuevo de `/Producer`.** Hoy nada escribe `/Producer`
   automáticamente; este batch no lo empieza a hacer — el campo queda como dato
   puramente editable por el usuario, sin que la app se autopromocione ahí.

## Tareas

### Fase 1 — Modelo (`core/pdf-document`)
- [x] T-167 Módulo `metadata.rs`: `DocumentInfo { title, author, subject, keywords,
      creator, producer: Option<String>, creation_date, mod_date: Option<PdfDate> }`
      (decisión 3). `PdfDate`: parse + format de `D:YYYYMMDDHHmmSSOHH'mm'` con test de
      round-trip (decisión 4). [MetadataModel]
- [x] T-168 `Command::SetDocumentInfo { before: DocumentInfo, after: DocumentInfo }` en
      `edit_log.rs` (decisión 5), `#[non_exhaustive]` en `Command`, cero breaking
      change. Apply/inverse + tests: `inverse().inverse() == self`, round-trip
      undo/redo, aplicar es inerte sobre `Document` (mismo criterio que B21 decisión 2
      — el log es la fuente de verdad, `pdf-save` lo replay al guardar), y que un log
      mixto metadatos+anotación/contenido se deshaga en orden. [MetadataModel, UndoRedo]
- [x] T-169 Lectura lazy: `LopdfDocument::document_info(&self) -> pdf_document::DocumentInfo`
      en `core/pdf-manip` (no en `pdf-document` — esa crate se mantiene sin lopdf; y no
      `pdf_document::read_document_info` como decía este ficha originalmente), snapshot
      del `/Info` actual — no se cuelga de `Document`/`LopdfDocument` (mismo criterio
      lazy que `PageContent` de B21/T-149). Reusa el `decode_pdf_text_string`
      (UTF-16BE+BOM / PDFDocEncoding) que ya tenía `LopdfDocument::info()` — un
      `DocumentInfo` de 4 campos preexistente, sin relación con el de este batch, que
      sigue intacto porque respalda la propiedades read-only ya shippeada del shell
      Linux. [MetadataModel]

### Fase 2 — Serialización (`core/pdf-save`)
- [x] T-170 `metadata.rs`: aplica `SetDocumentInfo.after` al `/Info` dict. Precedencia
      de `ModDate` explícito sobre `set_mod_date` (decisión 6). Codificación
      PDFDocEncoding/UTF-16BE+BOM según el texto (decisión 7). Cableado en
      `save_full_rewrite` únicamente — `save_incremental` es T-171. [MetadataSave]
- [x] T-171 Cierra el diferido de `strategy.rs:256-264` en `save_incremental`: clona
      `/Info` en `new_document` antes de mutar, solo cuando hay `SetDocumentInfo`
      pendiente (decisión 8). Sin ese comando, el incremental sigue sin tocar `/Info`
      — test de regresión que fija el comportamiento actual intacto.
      `apply_document_info`/`info_dict_mut` pasaron a ser genéricos sobre `ObjectSink`
      (como `forms.rs`/`annotations.rs`), que ganó `trailer()`/`trailer_mut()`; el
      clone-before-mutate sale gratis reusando `page_dict_mut` para el id del `/Info`.
      Dos tests de integración nuevos en `save_roundtrip.rs` contra un archivo real.
      [MetadataSave]
- [ ] T-172 Test de regresión: guardar sin ningún `SetDocumentInfo` en el log es
      byte-idéntico a hoy (ningún campo nuevo se escribe motu proprio). [Parity]

### Fase 3 — FFI (`core/pdf-ffi`)
- [ ] T-173 `FfiDocumentInfo`/`FfiPdfDate` en `types.rs`; `FfiEditCommand` gana
      `SetDocumentInfo`; `DocumentHandle::read_document_info() -> FfiDocumentInfo`
      (carga lazy). Smoke test: editar título + fecha de creación → `save_to_bytes` →
      reabrir → `read_document_info` devuelve los valores nuevos. [MetadataFFI]

### Fase 4 — Fixtures e interop
- [ ] T-174 Fixtures: un PDF con `/Info` completo (los siete campos + fechas válidas),
      uno sin `/Info` (documento mínimo/nuevo), uno con texto no-Latin1 en `/Title`
      (para fijar el camino UTF-16BE). Validador externo (pypdf o exiftool) que
      confirma los valores tras guardar. [MetadataFixtures, Parity]

### Fase 5 — Docs
- [x] T-175 README: fila nueva "Edit metadata" en la tabla de Tools & features, estado
      🔮 Planned (hecho en el mismo cambio que crea este documento).

## Tareas de UI (agregar a la ficha de B8, docs/batches-b8-b13.md, cuando arranque)

- [ ] T-176 (dep B22) Panel "Propiedades del documento": campos editables para los
      siete campos de texto + selector de fecha para Creation/ModDate; guarda vía
      `SetDocumentInfo`. Mismo patrón de panel lateral que ya usan formularios (B20).
      [MetadataUI]

## Criterios de aceptación

- Editar cualquiera de los siete campos de texto y guardar → el PDF resultante muestra
  el valor nuevo en Acrobat/Preview/`exiftool`.
- Vaciar un campo en la UI borra la clave del `/Info` dict al guardar, no escribe un
  string vacío.
- Texto no representable en PDFDocEncoding se guarda igual, vía UTF-16BE+BOM — nunca
  se rechaza una edición de metadatos por el contenido del texto.
- Editar `CreationDate`/`ModDate` explícitamente persiste ese valor en el guardado que
  lo incluye; un guardado posterior sin el comando vuelve al auto-stamp de `ModDate` de
  siempre (comportamiento hoy sin cambios para el caso común).
- Guardar sin tocar metadatos es byte-idéntico a hoy, en full rewrite e incremental.
- Undo/redo completo desde el día uno para `SetDocumentInfo`.

## Fuera de scope (v1)

Propiedades de sistema de archivos (Propietario, Compartido con, Equipo, Atributos,
fechas de filesystem) — no son datos del PDF, requieren APIs nativas privilegiadas por
SO y no existen como concepto en storage sandboxeado de iOS/Android (ver nota de scope
arriba) · sincronización con el stream XMP (`/Metadata`) — herramientas modernas
(Acrobat DC+) pueden priorizar XMP sobre el Info Dictionary clásico para mostrar el
título; v1 solo edita el Info Dictionary, que sigue siendo lo que todo visor
spec-compliant lee como mínimo común · `/Trapped` y claves custom arbitrarias del
`/Info` dict.

## Orden de ejecución

Fase 1 → 2 → 3 → 4 lineal (cada fase compila y testea sola, TDD estricto — igual
criterio que B20/B21). La UI (T-176) arranca cuando T-173 (FFI) esté listo, y solo
después de que termine el resto de batches de núcleo pendientes (secuencia explícita
del usuario: B22 es el último batch de núcleo antes de que el foco pase a UI de
shells).
