# Fichas de batches B8–B13 — pdf-editor-mvp

> Extraído de los artefactos SDD en engram (2026-07-13): tasks (`sdd/pdf-editor-mvp/tasks`),
> spec (`sdd/pdf-editor-mvp/spec`, obs 2248) y apply-progress (`sdd/pdf-editor-mvp/apply-progress`).
> Estado de partida: B0–B7 completos y verificados; los hallazgos del verify de B7 están todos
> resueltos salvo el WARNING de UX de re-render, diferido por decisión a B8/B9.

## Estado de dependencias (2026-07-13)

| Batch | Depende de | Estado |
|-------|-----------|--------|
| B8 (GTK4) | B6 ✓ (dep directa de crates, bypasea FFI) | **LISTO** |
| B9 (macOS) | B7 ✓ | **LISTO** (paralelo con B10) |
| B10 (Windows) | B7 ✓ + spike T-010 GO ✓ | **LISTO** (paralelo con B9) |
| B11 (verificación) | B8+B9+B10; T-123/T-125 además B15–B19; T-124 además B12 | BLOQUEADO |
| B12 (pdf-sign) | B6 ✓ | **LISTO** (paralelo a todos los shells) |
| B13 (Android) | B7 ✓ (NO depende de B1) | **LISTO** |
| B20 (formularios AcroForm, ver [batch-forms.md](batch-forms.md)) | B6 ✓ | **LISTO** (paralelo a todos los shells; T-141..T-143 de B8 dependen de él) |
| B21 (edición de contenido de página, ver [batch-content-edit.md](batch-content-edit.md)) | B6 ✓ | **LISTO** (paralelo a todos los shells; T-161..T-163 de B8 dependen de él) |

B8, B9, B10, B12 y B13 son mutuamente independientes y pueden correr en paralelo.
B14 (iOS) queda fuera de este documento: depende de B9 (reutiliza sus bindings y vistas).

---

## B8 — Shell GTK4 (Linux, dogfood)

**Dependencias:** B6 ✓. Dep directa de crates core — **bypasea B7/FFI** por decisión de design
("GTK4 FFI bypass"). El gap de `/Annots` preexistentes que lo bloqueaba se resolvió en la
remediación pre-B7. Las tareas de formularios T-141–T-143 además requieren B20
([batch-forms.md](batch-forms.md)); las de edición de contenido T-161–T-163 además requieren
B21 ([batch-content-edit.md](batch-content-edit.md)); el resto de B8 no está gateado por
ninguno de los dos.

### Tareas
- [x] T-044 Abrir/render página 1, scroll continuo, zoom fit-width/page/custom. [OpenPDF, NavZoom]
      **(2026-07-30 — completo: viewer multipágina async, scroll continuo, fit-width,
      fit-page y los controles de zoom custom existentes.)**
- [x] T-045 Prompt de contraseña en apertura encriptada + UX de error. [PwdPDF]
- [x] T-046 Selección de texto + búsqueda doc-wide con matches resaltados y navegables. [TextSelSearch]
      **(2026-07-19 — parcial: búsqueda doc-wide + matches navegables hechos; selección de texto pendiente)**
      **(2026-07-30 — completo y verificado a mano bajo WSLg. La geometría vive en
      `pdf_render::selection` (caret hit-test línea-primero, unión de rects por línea,
      transformación PDF↔pantalla) con 22 tests que corren en cualquier host, porque los
      cuatro shells la comparten — T-086 de Android NO debe reimplementarla. El shell aporta
      `app/selection.rs`: capa `DrawingArea` por página con `GestureDrag`, carga async de
      `text_runs`, y copia al portapapeles. El resaltado de matches no existía —la búsqueda
      solo hacía scroll— y se cierra con la misma capa vía `line_rects`.
      Gate en Linux: build + `clippy -D warnings` + suite completa del workspace, todo verde.**

      **Dos gotchas que costaron tiempo y valen para el resto de B8:**
      **(1) Ctrl+C NO puede colgar de un `EventControllerKey` en la ventana: corre en fase
      bubble, así que el `Entry` de búsqueda se lo come cuando tiene el foco. Va como
      `gio::SimpleAction` + `set_accels_for_action("win.copy", ["<Control>c"])`, resuelto por
      el shortcut manager y no por la cadena de foco. T-052 debe extender ESE mecanismo.
      (2) `Overlay` pinta en orden de agregado y el pipeline de tiles agrega bitmaps opacos al
      hacer zoom, así que la capa de resaltado se re-eleva tras cada lote
      (`selection::raise_highlights`) o los resaltados desaparecen en páginas tileadas.)**

      **(2026-07-31 — permiso de extracción respetado. Ambos caminos que entregan texto al
      usuario —búsqueda y copia de la selección— consultan `Viewer::text_extraction_refusal()`
      antes de trabajar. La regla vive una sola vez, en `pdf_manip::security`, y la comparten
      este shell y la frontera `pdf-ffi` que cruzan los demás; B8 bypasea FFI, así que sin
      esto Linux copiaba texto de documentos que lo prohíben mientras macOS/Windows/Android lo
      rechazaban. Un `/P` ilegible NO se asume permisivo, pero tampoco impide *ver* el
      documento.)**
- [x] T-047 Toolbar de anotaciones wired a pdf-annotate (7 tipos). [AnnoCreate, AnnoEditDelete]
      **(2026-07-31 — completo: los 7 tipos se crean por puntero, se editan y se borran contra
      el `EditLog`, con permisos respetados. La creación es en dos pasos —armar la herramienta
      (`ToggleButton`, una sola a la vez) y después dibujarla sobre la página—; el arrastre lo
      capta el mismo `GestureDrag` que ya hacía selección de texto, y una herramienta armada se
      queda con el gesto. La geometría sale de donde el usuario efectivamente arrastró: rect
      normalizado en las cuatro direcciones, polilínea completa para Ink, y un click (arrastre
      < 8pt en cualquier eje) cae a un tamaño por defecto anclado en el punto — nunca una
      astilla de tamaño cero, que sería irrecuperable porque borrarla exige seleccionarla.
      El preview durante el arrastre pasa por el MISMO `draw_annotation` que una anotación ya
      commiteada, así que lo que se arrastra es lo que queda. `app/annotations.rs` +
      `app/selection.rs`. Gate en Linux (WSL): `fmt --check` + `clippy --workspace
      --all-targets -D warnings` + `cargo test --workspace` (391 tests, 0 fallos).)**

      **Queda fuera, y a propósito:** seleccionar una anotación clickeándola en el canvas (hoy
      se cicla con "Previous annotation", que las alcanza todas) y los handles de arrastre para
      move/resize (hoy son botones con incremento fijo). Ninguna de las dos bloquea el criterio
      de aceptación de este batch.

      **El bug que casi se escapa, y que vale para los otros tres shells: `pdf_manip::open_document`
      NO sirve como fuente de permisos.** Un documento con **user password vacío** —el famoso
      "abre sin prompt pero restringe"— es descifrado in-place por la carga no autenticada de
      lopdf, que además borra `/Encrypt` del trailer. `open_document` lo ve como no cifrado y
      devuelve `SecurityContext: None`, y `None` significa "sin restricciones" para todos los
      gates. Es exactamente la clase de regresión que aecac3f arregló para copiar texto,
      reintroducida para anotaciones. La única fuente válida es `read_security_context`
      (el probe), que recupera los `/P` reales desde el `EncryptionState` decodificado.
      `core/pdf-manip/tests/annotation_permission.rs` clava las dos mitades del contrato.

      **Otros dos aprendizajes de la revisión:**
      **(1) `AnnotationSet` garantiza orden** (lo dice su propio doc: output determinista para
      el check byte-idéntico de CI). `ReplaceAnnotation` hacía remove+insert, o sea mandaba la
      anotación al final en cada move/resize/restyle: cambiaba el orden de pintado y dejaba el
      undo sin poder restaurar la posición. Ahora hay `AnnotationSet::replace` in-place —
      cualquier comando futuro que preserve identidad debe usarlo.
      **(2) "Prohibido" y "no se pudo cargar" son hechos distintos.** `AnnotationAccess` tiene
      `Forbidden` y `Unavailable` separados por eso, igual que `TextAccess::Unreadable`:
      colapsarlos hace que el shell le atribuya al documento una restricción que nunca declaró.

- [x] T-048 Keybindings undo/redo → EditLog. [UndoRedo]
- [x] T-049 GtkPrintOperation usando render_page a DPI de impresión. [Print]
- [x] T-050 gdk::Clipboard paste → stamp_from_image_bytes; rechazar URL-texto, sin fetch. [Clipboard]
- [x] T-051 Drag-and-drop: abrir PDF / insertar imagen como stamp. [ShortcutsDnD]
- [x] T-052 Shortcuts estándar C/V/Z/Y/P/S/F/O/N. [ShortcutsDnD]
- [x] T-053 Bundling pdfium .so + empaquetado (deb/AppImage). [pdfium dist]
      **(2026-08-11 — completo: `scripts/package-linux.sh` arma .deb y AppImage desde un
      tarball de PDFium verificado (checksum, args.gn Linux/x64/non-V8/non-XFA, chequeo ELF,
      sin symlinks) y falla cerrado antes de publicar nada; `verify-linux-package.sh`
      smoke-renderea dentro de cada paquete en un namespace aislado de red para probar que
      el PDFium empaquetado funciona y que nada llama a casa. El launcher apunta al binario
      empaquetado vía `PDFIUM_DYNAMIC_LIB_PATH`.)**
- [x] T-054 linux.yml CI: build + package. [infra]
      **(2026-08-14 — completo: `.github/workflows/linux.yml`, un job dedicado que
      construye el binario release, empaqueta con `scripts/package-linux.sh` y verifica
      con `scripts/verify-linux-package.sh` — mismos scripts que T-053 ya dejó
      fail-closed, esto solo los conecta a CI. `linux-gtk-ui.yml` queda intacto como
      suite aparte (tests de interacción bajo Xvfb); este job nunca toca un display,
      porque `verify-linux-package.sh` maneja el binario empaquetado vía
      `--package-smoke`, que rinde sin el loop de eventos de GTK. El smoke corre bajo
      `unshare --net` propio del script — de ahí el `sudo` que envuelve todo el paso,
      mismo motivo que el job `zero-network` de `core.yml`. Sin paso de upload: sigue
      sin haber proceso de publicación, igual que macOS/Windows a esta altura.)**
- [ ] T-141 (dep B20) Modo edición de formularios: colocar campo (texto/checkbox/radio/
      dropdown) sobre el canvas, arrastrar/resize con handles, inspector de estilo
      (fuente standard-14, tamaño, color) → comandos MoveFormField/ResizeFormField/
      RestyleFormField del EditLog. [FormUI, UndoRedo]
- [ ] T-142 (dep B20) Modo relleno: panel lateral de inputs generado desde
      `list_form_fields` (o el FormFieldSet directo — B8 bypasea FFI); tipear emite
      SetFieldValue y dibuja el valor como overlay en vivo sobre el rect del widget
      (sin re-render pdfium; /V + /AP se escriben al guardar). [FormUI]
- [ ] T-143 (dep B20) Tab-order del panel = orden del FormFieldSet; foco en el input
      del panel resalta el widget correspondiente en el canvas y viceversa. [FormUI]
- [x] T-161 (dep B21) Modo edición de contenido en el canvas: click sobre un text run
      existente abre un editor inline que preserva fuente/tamaño/posición; los runs no
      editables por `EncodingGap` se muestran distinguibles con explicación al usuario —
      nunca un fallo silencioso ni un intento que corrompa el stream. [ContentEdit]
      **(2026-08-14 — completo: `app/content_edit/` nuevo (mod/model/editor/command),
      wired directo a `pdf-edit` (bypass FFI, igual que anotaciones). Un toggle "Edit
      content" (mutuamente excluyente con las herramientas de anotación, en ambas
      direcciones) arma el modo; el mismo `GestureDrag` por página que ya usan selección y
      anotaciones colapsa un arrastre corto en un click (mismo umbral que T-047), que
      hit-testea contra `PageContent` parseado on-demand vía `pdf_edit::read_page_content`
      — síncrono, no hace falta el patrón async de `characters` porque no cruza a pdfium.
      El editor inline es un `gtk::Entry` real como tercer hijo del `Overlay` de la página,
      posicionado con márgenes desde `place_rect` — primer widget-sobre-coordenadas-de-página
      del shell (todo lo demás son anotaciones pintadas, nunca widgets).

      **Validar antes de grabar, no después:** al confirmar (Enter/focus-out) el shell
      clona el `lopdf::Document` base y corre el `pdf_edit::replace_text_run` real sobre el
      clon — la misma llamada que `pdf-save` hará de verdad al guardar — y solo si acepta
      graba `Command::ReplaceTextRunContent` en el `EditLog`. `Command::apply` para todo
      comando de contenido es un no-op sobre `Document` (el modelo es un snapshot, no
      estado cacheado), así que validar solo al guardar habría hecho fallar el guardado
      ENTERO por un solo edit malo entre varios en cola. Un `EncodingGap` o
      `CompositeFontNotEditable` deja el editor abierto con el texto tipeado intacto — nunca
      se pierde ni se escribe nada. Los runs de fuente compuesta (`FontKind::EmbeddedComposite`)
      se conocen sin intentar nada (el encoder los rechaza siempre) y se pintan con contorno
      distinguible (guiones, color distinto) apenas se arma el modo — `content_edit::
      load_all_page_content` parsea todas las páginas de una (ya existen todos los
      `PageSlot`, el render es lo único virtualizado) para que el contorno no dependa de
      haber clickeado antes.

      **Gap encontrado en la implementación, no en el plan original:** editar contenido de
      página estaba gateado por el mismo permiso que anotaciones (`ANNOTATE`, bit 6 de
      `/P`), que es el bit equivocado — el PDF distingue "modificar anotaciones" de
      "modificar el contenido del documento" (bit 4). Se agregó
      `pdf_manip::content_editing_is_allowed` (`core/pdf-manip/src/security.rs`, espejo
      exacto de `annotation_editing_is_allowed`) y `ContentEditAccess` en el shell (espejo
      de `AnnotationAccess`) para que un documento que permite anotar pero prohíbe editar
      contenido (o al revés) no se reporte mal — la misma clase de regresión de permisos
      que este repo ya pisó dos veces (texto en T-046, anotaciones en T-047).

      **Deliberadamente diferido a T-163, no un descuido:** el WARNING de UX de re-render
      (el canvas sigue mostrando el bitmap viejo de pdfium hasta guardar+reabrir) sigue
      diferido acá, tal como lo deja escrito la ficha de T-163 — el status dice "pending
      save" igual que undo/redo. Verificado: build + `clippy --workspace --all-targets -D
      warnings` + `cargo test --workspace` en WSL, todo verde salvo un fallo preexistente
      de `package_smoke` (hash de píxeles) ya presente en `main`, no relacionado.

      **Pasada manual bajo WSLg, hecha (2026-08-14):** el shell WSLg SÍ es controlable
      (queda bajo el proceso `msrdc.exe`, no bajo el propio WSL/terminal — pedir acceso a
      ese proceso lo destapa). Con el sample embebido: armar "Edit content" pinta el
      contorno verde en cada run sin necesitar click previo; click en "A sample document"
      abre el `Entry` inline exactamente sobre el bbox del run con el texto preseleccionado;
      retipear y Enter cierra el editor y el status dice "Text updated. Changes are pending
      save." (el bitmap se queda viejo, como se documentó arriba); Ctrl+Z (el mismo botón
      Undo de anotaciones) dice "Edit undone. Changes are pending save." y revierte. Único
      hallazgo, ajeno al feature: la acción de tipeo del automatizador pega vía portapapeles,
      y el `Ctrl+V` de eso lo intercepta la acción `win.paste` (stamp de imagen) antes de
      llegar al `Entry` con foco — no reproducible con teclas reales sueltas, y probablemente
      afecta cualquier `Entry` de la app (incluida la búsqueda), no algo que este batch haya
      introducido; queda para revisar aparte, no bloquea T-161.)**
- [x] T-162 (dep B21) Imágenes de página real: seleccionar/mover/redimensionar con
      handles/reemplazar (file picker)/borrar imágenes existentes — distinto del stamp de
      anotación que ya existe (T-047). [ContentEdit]
      **Slice 1 (PR #76, mergeado 2026-08-15):** select/move/resize/delete, reusando el
      module split y la postura validate-before-record de T-161 (`content_edit::image`
      es el twin de `annotations::gesture`, `geometry.rs` un fork deliberado del de
      anotaciones — las imágenes son contenido de página, no `document.annotations`).
      Dos bugs de correctness encontrados y cerrados antes de mergear: (1) `finish_image_drag`/
      `delete_selected` no refrescaban el snapshot cacheado de la imagen tras un commit
      exitoso — como `replay_content_edits` (`core/pdf-save/src/content.rs`) resuelve los
      comandos en cola secuencialmente contra estado progresivamente mutado, un segundo
      edit sobre la misma imagen aún seleccionada quedaba con el bbox pre-edit y fallaba a
      resolver en el save, tumbando el save ENTERO. Se agregó
      `command::image_already_edited` como guardia: rechaza un segundo edit sobre una
      imagen que ya tiene un comando en el `EditLog`, con mensaje claro ("save and reopen
      before editing it again") en vez de dejar que llegue al save. (2)
      `begin_image_drag` no llamaba `editor::commit` antes de fijar `selected_image`,
      rompiendo la exclusión mutua con el editor de texto inline.
      **Slice 2 (esta sesión, 2026-08-15):** reemplazar imagen vía file picker.
      `Command::ReplaceImageSource` ya existía completo en el core desde T-150/T-154
      (`core/pdf-document/src/edit_log.rs`, `core/pdf-edit/src/edit.rs::replace_image_source`,
      replay en `core/pdf-save/src/content.rs`) — lo único que faltaba era la UI del shell.
      **Gap real encontrado, no solo wiring:** `Command::ReplaceImageSource.before: Vec<u8>`
      tiene que ser bytes que `pdf_edit::replace_image_source` pueda decodificar de nuevo
      (`image::load_from_memory`, vía `insert::image_xobject`) porque `inverse()` reusa el
      mismo camino forward con `before`/`after` invertidos — y los bytes crudos del XObject
      actual (`FlateDecode` de samples `DeviceRGB`/`DeviceGray`, sin cabecera de archivo) NO
      son ese formato. Se agregó `pdf_edit::image_source_bytes` (nueva función pública,
      `core/pdf-edit/src/edit.rs`) que lee el stream de la imagen actual — soporta
      `DCTDecode` (JPEG, bytes ya reutilizables tal cual) y muestras de 8 bits
      `DeviceGray`/`DeviceRGB` (sin filtro, o `FlateDecode`/`LZWDecode`/`ASCII85Decode`),
      con `/SMask` opcional para alpha — y las re-codifica como PNG. Cualquier otra cosa
      (`Indexed`, `DeviceCMYK`, >8 bpc, `CCITTFaxDecode`, `JBIG2Decode`, `JPXDecode`, un
      `/SMask` que no matchea) se rechaza con el nuevo `EditError::ImageSourceNotRecoverable`
      — misma postura que `EncodingGap` para texto: nunca se adivina, se rechaza el replace
      entero antes de escribir nada en vez de grabar un comando que el undo no podría
      restaurar. **El criterio de "soportado" no es "puedo leer estos bytes" sino "¿el undo
      restaura la MISMA imagen?"**, porque el `before` vuelve por
      `replace_image_source`/`insert::image_xobject`: por eso el Code Review agregó tres
      rechazos más que faltaban y habrían corrompido en silencio — un `/SMask` sobre un
      stream `DCTDecode` (un JPEG no tiene canal alpha propio, así que devolver el JPEG solo
      hacía que el undo restaurara la imagen opaca), un `/Decode` (remapea cada muestra:
      `[1 0 1 0 1 0]` pinta invertido, y el PNG re-codificado no lo registra) y un `/Mask`
      (transparencia stencil/color-key que las muestras no cargan). Además el read-back
      ahora exige que cada plano traiga exactamente `width*height*components` muestras
      (antes el plano del `/SMask` no se verificaba), valida `/Width`x`/Height` como
      positivos y dentro de `u32` con aritmética chequeada (un `/Width` negativo daba la
      vuelta en el cast y podía desbordar el conteo de muestras), y decodifica con techo
      derivado de esas dimensiones en vez del read sin límite de `get_plain_content` —
      misma postura anti-bomba que `parse::filter`'s `MAX_PAGE_CONTENT_BYTES`.
      El shell (`content_edit::command::current_source_bytes`/`validate_replace`,
      `content_edit::image::replace_selected`/`apply_replacement`) reusa exactamente la
      guardia `image_already_edited` del Slice 1 y el patrón take-then-put-back de
      `delete_selected`. Botón "Replace image" nuevo junto a "Delete image", mismo gate de
      sensibilidad. `pdf-edit`/`pdf-document`/`pdf-save` (crates cross-platform, sin gate de
      `target_os`) build+test+clippy verdes en Windows; el shell `linux-gtk` en sí queda sin
      verificar localmente por la misma razón de siempre (no compila fuera de Linux) —
      pendiente de CI.
- [x] T-163 (dep B21) Insertar texto/imagen nuevos como contenido de página real; y el
      ciclo save→reopen→re-render obligatorio tras cualquier commit de edición de contenido
      — acá el WARNING de UX diferido en T-046/T-047 deja de ser diferible: es el camino
      principal de la feature, no un caso de borde. [ContentEdit]
      **(2026-08-17 — completo.) Parte A — insertar:** dos toggles nuevos junto a "Edit
      content" (`insert_text_button`/`insert_image_button`, `ContentInsertKind` en
      `state.rs`), mutuamente excluyentes entre sí y con las herramientas de anotación
      (armar cualquiera de los dos arma "Edit content" como efecto colateral, porque el
      click solo llega al ruteo de inserción con `content_edit_mode` en `true`). Armado,
      un click en cualquier punto de la página —sin consultar `text_run_at`— abre un
      `Entry` en blanco (texto) o un file picker (imagen) anclado ahí. Texto usa una caja
      fija de 150×14pt cuyo borde inferior-izquierdo cae en el punto clickeado (consistente
      con `insert_text_run`'s `baseline = bbox.y + 0.25*size`); imagen reusa
      `pdf_annotate::stamp_placement`/`DEFAULT_STAMP_MAX_SIDE_PT` — el mismo default que ya
      usa el Stamp de anotaciones — en vez de inventar un segundo heurístico de tamaño. El
      nombre de recurso (`FInsN`/`XInsN`) lo elige `content_edit::model::
      unused_font_resource_name`/`unused_xobject_resource_name`, comparando solo contra los
      runs/imágenes que `PageContent` ya expone: reusar un nombre ya tomado haría que
      `ensure_font_resource` reciclara en silencio el recurso equivocado.

      **Parte B — el WARNING deja de ser diferible, para TODO commit de contenido, no solo
      inserts.** Nuevo `document::refresh_after_content_edit(viewer, message)`: guardado a
      buffer en memoria (nunca a disco) + reapertura, calcado de
      `spawn_save`/`save_snapshot_and_reopen` salvo dos diferencias deliberadas — sin
      destino/diálogo, y sin `confirm_signature_loss` (la firma se invalida en silencio con
      `ProceedAndInvalidate`, porque nada se escribe a un archivo real todavía; el save a
      disco de verdad sigue preguntando antes de tocar un byte). Reusa
      `prepare_reopened_session`/`session_matches`/`SessionToken` tal cual, así que una
      segunda edición que llega antes de que la primera reabra se coalesce solo — el mismo
      mecanismo que ya protegía el save a disco. Los seis sitios de commit de
      `content_edit::editor::commit`/`content_edit::image` (replace, move, resize, delete,
      replace-source, insert-text, insert-image) llaman esto en vez de escribir "Changes are
      pending save."; `annotations::command::history` (el undo/redo compartido de TODO el
      `EditLog`) lo llama también cuando el comando que se movió es uno de contenido —
      `Command::is_content_edit()` (nuevo, `core/pdf-document/src/edit_log.rs`) clasifica
      qué acaba de moverse, y `EditLog::peek_undo`/`peek_redo` (nuevos, de solo lectura) lo
      dejan mirar *antes* de aplicar el paso, porque `undo`/`redo` en sí solo devuelven si
      hubo paso, no qué comando fue.

      **El gap real, encontrado en code review:** el refresh reusaba `show_document` tal
      cual, y `show_document` está escrito para instalar un documento *distinto* — resetea
      todo lo que un documento nuevo debe resetear, incluidos `document_model` (y con él el
      `EditLog` entero) y `save_backing`. Como el refresh instala el *mismo* documento, eso
      rompía tres cosas de una: (1) cada edición de contenido borraba el historial de
      undo/redo completo, anotaciones previas incluidas, y dejaba muerta la rama
      `is_content_edit` recién agregada a `annotations::command::history`; (2) el
      `save_backing` pasaba a ser los bytes refrescados con log vacío, así que
      `will_invalidate_signatures` respondía `false` y el save a disco de un PDF firmado
      salía por `save_incremental` sin pasar nunca por `confirm_signature_loss` — la firma
      la había roto el propio refresh, en silencio; (3) `has_unsaved_changes` quedaba en
      `false` si el refresh fallaba, y abrir otro documento descartaba la edición sin
      preguntar.

      La corrección es tratar el refresh como lo que es —**un refresh de preview, no un
      open**—: `document::take_edit_state`/`restore_edit_state` levantan la mitad de la
      sesión que describe *lo que el usuario editó* (`document_model`, `save_backing`,
      `next_annotation_id`, `selected_annotation`) antes de `show_document` y la reponen
      después. Solo el handle de pdfium y los widgets de página se reemplazan de verdad.
      Con el `save_backing` original preservado, el log sigue keyed contra la base que lo
      validó, el save a disco sigue haciendo full rewrite y sigue preguntando por la firma,
      y el undo de un content edit ahora sí llega a `refresh_after_content_edit` como el
      diseño pretendía.

      La otra mitad que `show_document` reseteaba —con la misma lógica de "documento
      nuevo"— era **la posición de lectura**: zoom a `FitWidth` y scroll a 0 en *cada*
      commit de contenido, o sea que editar un run en la página 12 al 400% te devolvía al
      tope de la página 1. `take_view_state`/`restore_view_state` son el gemelo de los
      anteriores. El zoom se repone con `layout::set_zoom` (idempotente si ya coincide con
      el default). El scroll **no** se guarda como offset crudo sino como
      `layout::ReadingPosition` (página + fracción dentro de ella, funciones puras
      `reading_position`/`position_offset` con tests): un offset solo significa algo contra
      un `page_heights` concreto, y ese es justamente el vector que se recalcula. Se
      resuelve *después* del `set_zoom`, contra el stacking nuevo. Se aplica dos veces —ya
      mismo y en el próximo idle— porque `set_value` clampea contra el `upper` del
      adjustment y ese `upper` solo se pone al día con los widgets recién reconstruidos en
      el siguiente size-allocate; la primera aplicación evita el parpadeo por el tope de la
      página 1, la del idle es el fallback si la primera quedó clampeada, y solo scrollea
      hacia abajo para no pisar algo más reciente.

      `DocumentSession::unsaved_to_disk` (bool, default `false`) se mantiene, pero lo marca
      **quien registra el comando**, no el refresh que después alcanza al canvas: un refresh
      que falla deja la edición en el log igual, y un documento que se reporta limpio es uno
      que el guard de "abrir otro documento" descarta sin preguntar. `restore_edit_state` lo
      re-afirma al cruzar su propio reopen, porque los bytes que mostró no se escribieron a
      ningún lado. `has_pending_annotation_edits` se renombró a `has_unsaved_changes` y el
      `AlertDialog` de `confirm_replacing_edits` pasó a texto genérico ("Unsaved changes"),
      ya que ahora cubre contenido además de anotaciones.

      **Consecuencia de preservar la base original:** el `PageContent` que se re-parsea tras
      el refresh sale de esa base, no de los bytes en pantalla, así que contenido insertado
      desde el último save a disco se ve pero todavía no es clickeable — la misma limitación
      que `command::image_already_edited` ya declaraba ("save and reopen before editing it
      again"). Y como la base no ve las inserciones pendientes, elegir el nombre de recurso
      solo desde `PageContent` habría hecho que dos inserciones seguidas pidieran el mismo
      `XIns1`; `insert_image` rechaza un `source` bajo un nombre ya registrado
      (`ResourceNameInUse`) y, al pasar eso dentro de `replay_content_edits`, se llevaba
      puesto el save entero. `model::reserved_font_resource_names`/
      `reserved_xobject_resource_names` leen los nombres ya reclamados por comandos del log
      (incluido `RemoveImage`, que deja su XObject registrado para que el undo lo pueda
      repintar) y `unused_*_resource_name` los honra junto con los que la página ya pinta.

      Verificado: `cargo fmt --check` + `cargo build`/`test`/`clippy --all-targets
      --all-features -D warnings` en `pdf-document`, `pdf-edit`, `pdf-save`, `pdf-ffi`
      (todos cross-platform, sin gate de `target_os`) — todo verde en Windows, incluidos los
      tests nuevos de `Command::is_content_edit`/`EditLog::peek_undo`/`peek_redo` y de
      `content_edit::model::unused_font_resource_name`/`unused_xobject_resource_name`/
      `content_edit::command::validate_insert_text`/`validate_insert_image`. El shell
      `linux-gtk` en sí queda sin verificar localmente por la misma razón de siempre: en
      Windows `main.rs` excluye `mod app;` entero tras `#[cfg(target_os = "linux")]`, así
      que ni `cargo check -p linux-gtk` compila una sola línea del código de este batch —
      pendiente de CI. Revisión manual de todo el diff enfocada en un footgun conocido de
      Rust (temporal de `RefCell::borrow()` extendido por todo un `match`/`if let`): un
      `match viewer.state.borrow().content_insert_mode { ... }` original habría mantenido el
      `Ref` vivo mientras las ramas reabrían `viewer.state` — se corrigió leyendo a una
      variable local antes del `match`/`if`, mismo patrón que el resto del archivo ya usa en
      otros lados.

### Criterios de aceptación (spec)
- PDF válido sin cifrar abre y renderiza página 1.
- Contraseña errónea → error claro, sin crash y sin render parcial.
- Zoom al 150% persiste al navegar entre páginas.
- Búsqueda: "invoice" presente en 3 páginas → 3 matches resaltados y navegables.
- Anotación borrada + Ctrl+Z → restaurada desde su inverse data del EditLog; Ctrl+Y re-aplica.
- El strip de protección NO es deshacible (vive en audit log, jamás en EditLog).
- Paste de bitmap → stamp en la posición de pegado; paste de URL-texto → mensaje informativo
  y CERO llamadas de red.
- Imprimir un rango → diálogo nativo GTK, output coincide con el documento renderizado.
- PDF arrastrado a la ventana abre; imagen arrastrada sobre un documento abierto → stamp en
  el punto de drop.

### Gotchas heredados
- El gap de undo de B5 (move/resize/restyle de anotaciones fuera del EditLog) se cierra AQUÍ —
  `Command` es `#[non_exhaustive]` precisamente para poder agregar esas variantes.
- B8 es donde se decide el WARNING de UX de re-render (edits sin guardar requieren
  `save_to_bytes` + reabrir handle) con datos de shell real, no en el vacío.

---

## B9 — Shell macOS (SwiftUI)

**Dependencias:** B7 ✓. Paralelo con B10.

### Tareas
- [ ] T-055 App SwiftUI vía bindings pdf-ffi; abrir/render/scroll/zoom. [OpenPDF, NavZoom]
- [ ] T-056 Prompt de contraseña, selección/búsqueda de texto, toolbar de anotaciones, undo/redo. [PwdPDF, TextSelSearch, AnnoCreate, UndoRedo]
- [ ] T-057 NSPrintOperation vía render_page. [Print]
- [ ] T-058 NSPasteboard paste + drag-and-drop; shortcuts. [Clipboard, ShortcutsDnD]
- [ ] T-059 Bundling .dylib, sign + notarize en macos.yml. [pdfium dist]

### Criterios de aceptación
- La misma batería funcional de B8 (OpenPDF, PwdPDF, NavZoom, TextSelSearch, AnnoCreate,
  UndoRedo, Print, Clipboard, ShortcutsDnD), ejecutada a través de la superficie FFI real.
- macos.yml produce una app firmada y notarizada.
  - **Estado parcial:** hoy macos.yml arma y verifica un artefacto de desarrollo con
    firma ad-hoc (necesaria para que corra en Apple Silicon), NO notarizado, y
    **no lo publica** — el bundle vive solo dentro del run. La firma de distribución
    + notarize + el upload siguen abiertos en T-059; el criterio no está cumplido.

### Notas
- macos.yml YA existe con el job `swift-bindings` (T-042) — B9 lo extiende, no crea workflow.
  El job pasó a llamarse `macos-development-artifact` y absorbió la generación de bindings:
  sigue fallando si falta `pdf_ffi.swift`, `pdf_ffiFFI.h` o `pdf_ffiFFI.modulemap`.
- T-055 está cubierto solo en su porción abrir/render/scroll/zoom. La UI real (contraseña,
  selección/búsqueda, anotaciones, undo/redo, print, clipboard) sigue sin empezar.
- **Piso de macOS = 12.0, no 11.0.** El PDFium 7763 pinneado (el mismo que comparten Linux,
  Windows y Android) declara `minos 12.0`. El gate fail-closed de `build-macos.sh` lo detectó
  al verificar el bundle real. Bajarlo de nuevo implica pinnear un PDFium distinto solo para
  macOS. Ningún doc del repo declaraba un piso antes de esto.
- La app se compila para la arquitectura del host (`ARCHS = $(NATIVE_ARCH_ACTUAL)`) porque
  cargo emite `libpdf_ffi.dylib` solo para el triple del host. Un `.app` universal exige
  `lipo` sobre el dylib de Rust — va junto con firma de distribución en T-059.
- Para guardar cifrado tras un edit estructural el shell DEBE ofrecer el flujo
  `openWithPasswords`/`openWithPasswordsFromBytes` (fix post-verify de B7); con una sola
  password solo hay save incremental — `save_to_bytes` devuelve `InvalidSaveRequest` tipado
  en el caso no soportado.
- B14 (iOS) reutilizará estos mismos bindings Swift y una porción sustancial de las vistas:
  diseñar las vistas SwiftUI con esa reutilización en mente.

---

## B10 — Shell Windows (WinUI3 + C#)

**Dependencias:** B7 ✓ + T-010 spike uniffi-bindgen-cs = GO ✓. Paralelo con B9.

### Tareas
- [x] T-060 App WinUI3 vía bindings C# de uniffi-bindgen-cs; abrir/render/scroll/zoom. [OpenPDF, NavZoom]
      **(2026-07-28 — completo: zoom fit-width/fit-page/custom en `Pdf.Windows/Viewer/PageZoom.cs`,
      con escalera de pasos, techo de píxeles por página y anclaje del scroll al recomponer.
      La matemática es WinUI-free y está cubierta por 9 tests de la suite de facade)**
      **(2026-07-29 — zoom profundo por tiles: `Viewer/ViewportTilePlan.cs` ancla los tiles
      a una grilla fija en el espacio de píxeles de la página, `pdf_render::render_page_tiles`
      rasteriza todo el viewport en un solo job del actor con la página cargada una vez, y
      `PageZoom.BridgeDpi` le da a la página tileada un bitmap base puente (2 MP) en lugar de
      dejarla con el render del zoom anterior. Carril propio para los lotes de tiles en
      `PdfDocumentFacade`, para que no se cancelen contra el render de página completa)**
- [x] T-061 Prompt de contraseña, selección/búsqueda, toolbar de anotaciones, undo/redo. [PwdPDF, TextSelSearch, AnnoCreate, UndoRedo]
      **(2026-07-19 — parcial: prompt de contraseña + búsqueda con matches navegables hechos; anotaciones + undo/redo pendientes)**
      **(2026-08-04 — anotaciones + undo/redo cerrados en el PR #37 "reach annotation parity
      with the Linux shell" y los PR #44-47 subsiguientes (preview de stamp, paste/drop de
      imagen, canvas dedicado). Esta ficha había quedado desactualizada.)**
      **(2026-08-19 — completo: selección de texto arrastrando + copiar (Ctrl+C). El gap real
      era que `pdf-ffi` no exponía la geometría de selección — `pdf_render::selection`
      (caret hit-test línea-primero, unión de rects por línea) es la misma que usa el shell
      Linux, pero ese shell la consume linkeando `pdf-render` directo (bypass FFI); Windows
      no puede, todo pasa por la facade C# (`apps/windows/CONTRACT.md`: "no view calls
      generated bindings directly"). Se agregó `core/pdf-ffi/src/selection.rs`:
      `FfiPageCharacters`, un objeto de interfaz UniFFI que envuelve
      `pdf_render::PageCharacters` (`caret_at`/`text_in`/`rects_in`), más
      `DocumentHandle::page_characters(page_index)` con el mismo gate de permisos que
      `text_runs`/`search`. Ningún otro shell reimplementa el hit-test — sigue viviendo una
      sola vez en `pdf_render`.
      El lado C#: `IPdfCorePageCharacters`/`GeneratedPageCharacters` en la capa Facade,
      `PdfDocumentFacade.PageCharactersAsync` (despachada fuera del hilo de UI porque lee
      text runs de pdfium), y `MainWindow.Selection.cs` nuevo. La carga de `PageCharacters`
      es async (una vez por página, cacheada); una vez cargada, cada `PointerMoved` de un
      arrastre la consulta de forma síncrona — igual criterio que ya usa el hit-test de
      anotaciones, para no pagar un round-trip de facade por cada movimiento del mouse.
      **Orden de prioridad del gesto de puntero, igual que el shell Linux
      (`app/selection.rs`'s `begin_selection`):** una herramienta de anotación armada, o un
      press que cae sobre el handle/cuerpo de una anotación existente, reclama el gesto;
      si ninguna de las dos lo reclama —incluyendo cuando `AnnotationEditingAllowed` es
      falso, que antes cortaba toda interacción del canvas— el gesto cae a selección de
      texto. Esto exigió que `BeginAnnotationPointer`/`ContinueAnnotationPointer`/
      `EndAnnotationPointerAsync` devuelvan `bool` (¿reclamado?) en vez de `void`.
      Gate en Windows: MSBuild real de Visual Studio (`dotnet build` no compila este shell,
      ver Gotchas críticos) con 0 warnings/0 errores, y la suite completa de
      `Pdf.Windows.Facade.Tests` (79/79, incluye 2 tests nuevos de `PageCharactersAsync`)
      vía `dotnet run`. **No verificado a mano en la app corriendo**: el binario de
      desarrollo no está registrado como app de Start Menu, y las herramientas de control de
      escritorio de esta sesión sólo autorizan apps instaladas/registradas — arrastrar sobre
      texto y confirmar el resaltado + Ctrl+C queda pendiente de una pasada manual.)**
- [x] T-062 PrintDocument vía render_page. [Print]
- [ ] T-063 WinRT Clipboard/DataPackage paste + drag-and-drop; shortcuts. [Clipboard, ShortcutsDnD]
      **(2026-08-19 — parcial: el paste de bitmap del portapapeles y el drag-and-drop
      (abrir PDF / stamp de imagen) ya estaban resueltos por PRs anteriores
      (`MainWindow.FileDrop.cs`, T-049/T-050) — el gap real era los shortcuts. Del set
      C/V/Z/Y/P/S/F/O/N solo C/V/Z/Y estaban cableados. Se agregaron Ctrl+O/Ctrl+S/Ctrl+P
      como `KeyboardAccelerator` en los botones Open/Save/Print (WinUI invoca el `Click`
      del botón automáticamente cuando el accelerator no declara `Invoked`, el mismo patrón
      ya usado por Ctrl+Z/Ctrl+Y en Undo/Redo), Ctrl+F mueve el foco a `SearchBox` sin
      disparar la búsqueda (paridad con la acción `win.find` del shell GTK, que solo hace
      `grab_focus`), y Ctrl+N pide un documento nuevo.
      Ctrl+N expuso que `PdfDocumentFacade.CreateBlankAsync` no tenía forma de discardear
      ediciones pendientes — a diferencia de `OpenAsync`, no aceptaba `discardPendingEdits`,
      así que el flujo Save/Discard/Cancel no podía completar la rama Discard. Se le agregó
      el mismo parámetro que `OpenAsync` ya tiene. `MainWindow.xaml.cs` reusa el mismo guard
      de ediciones pendientes (`AskPendingEditDecisionAsync` + `ReportFailedOpen` +
      `ShowOpenedDocument`) que ya protegía Open, así que Ctrl+N no es un camino nuevo de
      pérdida de datos. No hay botón de "New" en la UI — el shell GTK tampoco lo tiene,
      Ctrl+N es shortcut-only en ambos.
      2 tests nuevos en `Pdf.Windows.Facade.Tests` cubren el guard de `CreateBlankAsync`
      (bloqueo con ediciones sin guardar + discard explícito). Gate: MSBuild real de Visual
      Studio (`dotnet build` no compila este shell) con 0 warnings/0 errores, y
      `Pdf.Windows.Facade.Tests` 83/83 vía `dotnet run`.

      **Verificado a mano en la app corriendo (2026-08-19, x64 Debug).** Andan: Ctrl+O
      (abre el file picker), Ctrl+S (picker + guardado real a disco) y el guard de
      ediciones pendientes de Ctrl+N con su rama Cancel. Sin probar todavía: Ctrl+F,
      Ctrl+P, la rama Discard de Ctrl+N y C/V/Z/Y.

      **Por qué queda sin marcar: Ctrl+N no produce un documento usable.** El
      `create_blank_document` del core crea un PDF de CERO páginas — el `PageSize`/
      `Orientation` son el default para páginas insertadas después, no una página que
      te crea — así que el lector cae en "This document has no pages." con toda la
      barra de anotaciones deshabilitada y sin forma de agregar una: este shell no
      expone inserción de páginas. El shell GTK es la referencia a igualar
      (`new_blank_document` en `apps/linux-gtk/src/app/document.rs`): encadena
      `create_blank_document` con `insert_blank_page` sobre el documento base, y así
      entrega una A4 real con la sesión limpia. Portarlo no es transcribir:
      `InsertBlankPage` llega a este shell sólo como `FfiEditCommand`, o sea que
      pasar por ahí abriría el documento nuevo ya con una edición sin guardar, que no
      es lo que hace GTK.

      Los 83 tests no lo ven, y vale entender por qué antes de confiar en ese número:
      `FakeCore.CreateBlank` devuelve el mismo `PageCount` distinto de cero que
      `OpenFromBytes`, así que el doble nunca modela el contrato de cero páginas del
      core real. Verde sobre un fake que miente.

      Arreglos de shell que salieron de la pasada manual y sí entran acá:
      `SetBusy` blanquea la barra de anotaciones en AMBOS bordes, y sólo
      `ShowOpenedDocument`/`ReportFailedOpen` la restauran, así que toda salida
      temprana la dejaba gris. Pasaba en tres lugares — `OpenDocumentAsync`,
      `NewDocument_Invoked` y `SaveToPickedFileAsync`, este último contradiciendo su
      propio "PDF saved. Annotations remain editable in this session." Ahora hay un
      helper `RestoreAnnotationControls()` en las salidas tempranas, y el save quedó
      como wrapper con `finally` sobre `PickDestinationAndWriteAsync` para que la
      invariante viva en un solo lugar. Los tres verificados a mano.)**
- [ ] T-064 Bundling .dll + firma Authenticode en windows.yml. [pdfium dist]
      **(2026-07-19 — parcial: build del shell WinUI en CI; firma Authenticode pendiente)**

### Criterios de aceptación
- Misma batería funcional que B8/B9 vía FFI.
- Los errores FFI llegan a C# como excepciones tipadas anidadas — nunca strings crudos
  (contrato validado en el spike T-008).
- windows.yml produce binario firmado con Authenticode.

### Gotchas críticos
- uniffi-bindgen-cs v0.11.0 exige `uniffi = "0.31.0"` EXACTO — ya pineado en
  `core/pdf-ffi/Cargo.toml`; cualquier bump debe actualizar ambos juntos
  (ver spikes/uniffi-cs/README.md).
- El round-trip de buffers ≥8MB cuesta ~20ms de marshaling (p50, hardware real del spike),
  NO "single-digit ms": presupuestarlo en el render loop (~1.5% del budget de 1.5s).
- Cargar una página NO es gratis: `document.pages().get()` es un `FPDF_LoadPage` que parsea
  el content stream. En una página con mucho texto a zoom profundo eso cuesta más que
  rasterizar un tile, así que los tiles de un viewport van SIEMPRE en un solo job del actor
  (`render_page_tiles`) — pedirlos de a uno paga el parseo N veces.
- El shell WinUI no compila con `dotnet build`: las tareas de empaquetado PRI que necesita
  solo las carga el MSBuild de Visual Studio (la CI lo resuelve con `vswhere`, ver
  windows.yml). Además el build falla con MSB3021/MSB3027 si hay una instancia de la app
  abierta bloqueando `bin/`.

---

## B11 — Verificación transversal

**Dependencias:** B8 + B9 + B10. Las tareas de firma (T-123, T-125) además requieren B15–B19;
T-124 requiere B12.

### Tareas
- [ ] T-065 Checklist manual de round-trip de anotaciones en Acrobat/Preview, pre-release. [AnnoInterop]
- [ ] T-066 Test de paridad estructural: misma op en 2 plataformas, comparar output. [Parity]
- [ ] T-067 Auditoría offline/zero-network completa en todos los shells. [Offline]
- [ ] T-123 (dep B15–B19) Round-trip cross-viewer de firmas criptográficas: Acrobat + un segundo validador independiente. [FirmaCripto, AnnoInterop]
- [ ] T-124 (dep B12) Corpus de PDFs firmados conocidos-buenos (rcgen self-signed), versionado en tests/fixtures: single-signature y segunda-firma-no-invalida-primera. [FirmaCripto]
- [ ] T-125 (dep B15–B19) Smoke tests del contrato CertificateSourcePort por plataforma (incl. variante Secure Enclave en macOS/iOS). [FirmaCripto]

### Criterios de aceptación (spec)
- Anotaciones creadas por la app renderizan en Acrobat/Preview con tipo, posición y contenido
  correctos. Text-note enlaza con `/Popup` + `/Parent` de vuelta — NUNCA `/IRT` (reservado
  para reply threads). Stamps renderizan vía su `/AP` con transparencia (SMask).
- Paridad = equivalencia ESTRUCTURAL en producción (páginas, contenido, anotaciones,
  semántica de metadata); byte-idéntica SOLO en CI con clock e ID-generator deterministas
  inyectados (los PDFs embeben /ModDate e IDs no deterministas por defecto).
- Cero requests salientes monitorizando la red durante cualquier operación MVP, en todos
  los shells.

---

## B12 — Crate pdf-sign

**Dependencias:** B6 ✓ únicamente. Conceptualmente paralelo a B7/B8/B9/B10/B13 — no depende
de ningún shell.

### Tareas
- [x] T-070 **PRIMERA TAREA DEL BATCH — smoke test**: validar un callback interface UniFFI
      SÍNCRONO CON VALOR DE RETORNO (patrón de `sign_digest`). El spike T-009 solo validó
      callbacks async de eventos sin retorno; hay que desriesgar ANTES de construir el resto
      del batch sobre este patrón. [risk-mitigation]
- [x] T-071 Scaffold `core/pdf-sign` con RustCrypto (`cms`, `x509-cert`/`x509-parser`, `der`,
      `spki`, `sha2`, traits `signature`), aislado del resto de core (un build futuro
      "solo visor" no debe arrastrar crypto). [infra]
- [x] T-072 Trait `CertificateSourcePort`: `list_identities() -> Vec<SigningIdentity>`,
      `sign_digest(identity_id, digest, alg) -> Result<Vec<u8>, SignError>`. [FirmaCripto]
- [x] T-073 Builder de campo AcroForm/Sig: placeholder `/Contents` (hex ceros, tamaño
      suficiente) + cálculo de `/ByteRange` (dos rangos alrededor del placeholder). [FirmaCripto]
- [x] T-074 Digest configurable (SHA-256/384/512) sobre los byte ranges. [FirmaCripto]
- [x] T-075 Builder CMS SignedData (PKCS#7): digest firmado + cadena de certificados. [FirmaCripto]
- [x] T-076 Hook hacia el writer incremental de pdf-save:
      `append_signature_bytes(doc_bytes, byte_range, signature_der) -> Vec<u8>`. [FirmaCripto, IncrementalAPI]
- [x] T-077a Adaptador Linux PKCS#11 (`cryptoki`) implementando `CertificateSourcePort`;
      la clave privada nunca sale del token. Crate hoja separado (p. ej.
      `core/pdf-sign-pkcs11`) — `pdf-sign` core no debe arrastrar implementaciones de
      clave privada. [FirmaCripto]
- [x] T-077b Adaptador archivos .p12/.pfx vía `p12-keystore` (Rust puro, mantenido;
      `pkcs12` de RustCrypto descartado: descifrado pendiente upstream, aún pre-release)
      + firma in-process (`rsa`/`p256` con traits `signature`). Mismo crate hoja o
      feature no-default — jamás en `pdf-sign` core. Depende de T-077a solo por
      compartir crate/contrato de tests. [FirmaCripto]
- [x] T-078 Corpus de fixtures firmados conocidos-buenos (rcgen self-signed, uso exclusivo
       de test). [FirmaCripto]
- [x] T-079 Cross-validación: verificación estructural automatizada (hash/ByteRange) en CI +
       checklist manual documentado para validación externa. [FirmaCripto]
- [x] T-080 Tests: corrección de ByteRange, sizing del placeholder, coincidencia de digest,
       segunda-firma-preserva-primera. [FirmaCripto]

### Criterios de aceptación (spec delta)
- Firma anexada EXCLUSIVAMENTE vía incremental update — jamás full rewrite al firmar
  (invalidaría `/ByteRange` de firmas previas).
- Segunda firma NO invalida la primera; ambas verificables independientemente.
- Al abrir un documento firmado: indicador básico pass/fail de validez de hash/estructura.
  Contenido alterado fuera del `/ByteRange` firmado → indicador "inválida" SIN bloquear la
  apertura del documento.
- Sin certificados disponibles en el almacén → mensaje claro, sin crash.
- Firma y verificación 100% offline: cero TSA (RFC 3161), OCSP o CRL por red.
- **Out of scope explícito:** LTV, timestamping, validación profunda de cadena de confianza,
  workflow UX de múltiples firmantes.

### Regla de oro (T-076)
El writer incremental YA existe: `pdf_save::strategy::save_incremental`/`IncrementalWriter`,
backed por `lopdf::IncrementalDocument`. pdf-sign lo REUSA vía un hook de bajo nivel a exponer
en pdf-save (o extendiendo `ObjectSink`) — construir un writer paralelo es un defecto de
arquitectura, no un atajo.

---

## B13 — Shell Android (Kotlin/Jetpack Compose)

**Dependencias:** B7 ✓ únicamente. NO depende de B1 (ese spike gateaba exclusivamente
Windows/C#). Paralelo con B9 y B10.

### Tareas
- [ ] T-081 Scaffold Kotlin/Compose + generación de bindings UniFFI Kotlin en android.yml. [infra]
      **(2026-07-26 — parcial: scaffold Gradle/Compose + `scripts/package-android.sh` genera los
      bindings Kotlin desde el `libpdf_ffi.so` construido; android.yml solo corre los unit tests
      JVM, no genera bindings)**
- [ ] T-082 Cross-compile pdfium .so por ABI (arm64-v8a, armeabi-v7a, x86_64) + empaquetado. [pdfium dist]
      **(2026-07-26 — parcial: cross-compile de pdf-ffi vía cargo-ndk para arm64-v8a y x86_64;
      armeabi-v7a pendiente. PDFium sigue siendo un input externo — el repo no lo vendorea ni
      lo descarga, ver apps/android/README.md)**
- [ ] T-083 SAF: `ContentResolver.openInputStream()` → `open_from_bytes`; guardar vía
      `OutputStream` del mismo Uri → `save_to_bytes`. [ui-android, FileAccessPort]
      **(2026-07-26 — parcial: apertura vía SAF (`OpenDocument`) → `open_from_bytes` hecha;
      el guardado no existe todavía porque no hay edición en el shell)**
- [ ] T-084 Abrir/render página 1, scroll continuo, zoom fit-width/page/custom. [ui-android, OpenPDF, NavZoom]
      **(2026-07-26 — parcial: lector de scroll continuo con ventana de render/caché,
      placeholders dimensionados por media box y fit-to-width con techo de píxeles por
      página + invalidación por rotación; zoom custom por botones, con escalera discreta,
      re-render a DPI escalado, límite de píxeles y scroll horizontal, hecho; zoom fit-page
      y pinch siguen pendientes)**
- [x] T-085 Prompt de contraseña en apertura encriptada + manejo de error. [ui-android, PwdPDF]
- [ ] T-086 Selección de texto + búsqueda doc-wide con matches navegables. [ui-android, TextSelSearch]
      **(2026-07-26 — parcial: búsqueda doc-wide + matches navegables hechos; selección de
      texto pendiente — mismo estado que T-046 en GTK4)**
      **(2026-07-30 — la matemática de selección ya está resuelta y testeada en
      `pdf_render::selection`; falta exponerla por `pdf-ffi` y escribir la mitad Compose.
      NO reimplementar el hit-test acá.)**
- [ ] T-087 Toolbar táctil wired a pdf-annotate (7 tipos incl. image stamp). [ui-android, AnnoCreate, AnnoEditDelete]
- [ ] T-088 Firma dibujada: trazo táctil → PNG con canal alfa → `stamp_from_image_bytes` en
      el placement_rect. [FirmaDibujada]
- [ ] T-089 Undo/redo vía botones táctiles → EditLog. [ui-android, UndoRedo]
- [ ] T-090 Android PrintManager usando render_page a DPI de impresión. [ui-android, Print]
      **(2026-07-26 — parcial: `PrintDocumentAdapter` wired a PrintManager, pero entrega los
      bytes originales del PDF al spooler en vez de rasterizar con `render_page` a DPI de
      impresión; tampoco honra el rango de páginas pedido. Vale mientras no haya edición —
      en cuanto el shell edite, imprimiría el documento sin los cambios)**
- [ ] T-091 Paste de bitmap desde portapapeles → stamp; rechazar URL-texto sin fetch. [ui-android, Clipboard]
- [ ] T-092 Equivalentes táctiles de drag-and-drop: share-sheet nativo / selector SAF /
      arrastre en split-screen. [ui-android, ShortcutsDnD]
- [ ] T-093 .apk firmado + android.yml CI completo (bindings Kotlin + cross-compile por ABI +
      Gradle + firma). [ui-android, infra]
      **(2026-07-26 — parcial: android.yml existe y corre `:app:testDebugUnitTest` (tests JVM
      puros, sin PDFium); falta build del .apk, cross-compile por ABI y firma. OJO: el CI
      pinea Gradle 8.11.1 mientras el wrapper commiteado pide 9.5.0 y el proyecto usa AGP
      9.3.1 — hay que unificar antes de ampliar el workflow)**

### Criterios de aceptación (spec delta)
- Acceso a archivos EXCLUSIVAMENTE vía Storage Access Framework: el core recibe
  bytes/descriptors, NUNCA rutas crudas — ni al abrir ni al guardar.
- Paridad funcional completa con desktop: los 7 tipos de anotación, undo/redo con el MISMO
  EditLog, impresión nativa (PrintManager), paste de bitmap como stamp.
- Touch-first: toolbar táctil reemplaza los atajos Ctrl/Cmd; share-sheet/SAF/split-screen
  reemplazan el drag-and-drop de escritorio (imagen arrastrada en split-screen → stamp).
- La firma dibujada persiste como annotation estándar (ink o image stamp con /AP) e
  interopera en Acrobat/Preview.
- android.yml produce un .apk firmado instalable en dispositivo compatible.

### Nota clave
T-068 ya está hecho: `open_from_bytes`/`save_to_bytes` son el contrato canónico en pdf-ffi,
incluida la variante dual-password (`open_with_passwords_from_bytes`) para guardar cifrado
tras edits estructurales.
