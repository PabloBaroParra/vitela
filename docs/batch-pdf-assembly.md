# Checklist de combinación y organización de PDFs en Linux

Este documento sigue la implementación de la importación de varios PDFs en un
documento abierto y su organización por documentos completos o por páginas
individuales. La primera entrega corresponde exclusivamente al shell GTK4 de
Linux; el comportamiento reutilizable debe permanecer en el núcleo Rust.

## Cómo actualizar este checklist

- Una tarea solo pasa a `[x]` cuando su implementación y sus pruebas están
  completas.
- Cada tarea terminada debe añadir una nota breve con fecha, archivos relevantes
  y evidencia de verificación.
- Los trabajos parciales permanecen en `[ ]` y se documentan como parciales.
- Los gates no ejecutados deben figurar como no verificados; nunca se asumen
  correctos por haber pasado otros gates.
- Las decisiones que cambien alcance, contratos o preservación de datos deben
  quedar registradas antes de continuar la implementación.

## Estado

| Área | Estado |
|------|--------|
| Modelo y operaciones | Completo |
| Importación PDF | Pendiente |
| Guardado y renderizado | Pendiente |
| Integración Linux | Pendiente |
| Vista por documentos | Completo |
| Vista por páginas | Completo |
| Animación | Parcial |
| Pruebas y gates | Pendiente |

## Decisiones cerradas

- [x] Limitar la primera implementación al shell GTK4 de Linux.
- [x] Permitir elegir entre organizar documentos completos y páginas.
- [x] Usar el modo por documentos como vista inicial.
- [x] Mantener exactamente el orden de las páginas al cambiar de vista.
- [x] Representar como bloques separados los tramos no contiguos de un mismo
  PDF.
- [x] Evitar que un cambio de vista modifique el documento o cree historial.
- [x] Preservar contenido PDF real; no rasterizar páginas importadas.

## Límites de responsabilidad

- `pdf-document` posee la identidad, procedencia y orden lógico de las páginas,
  además de los comandos reversibles.
- `pdf-manip` posee la copia y remapeo de objetos PDF entre documentos.
- `pdf-save` materializa el orden lógico final sobre el documento PDF.
- El shell Linux posee selección de archivos, diálogos, estado visual,
  animación y cableado de eventos.
- La UI no debe manipular directamente árboles de objetos PDF.
- El núcleo no debe depender de GTK, rutas seleccionadas por la UI ni widgets.

## 1. Modelo y operaciones

- [x] Añadir procedencia explícita para páginas del documento base, páginas en
  blanco y páginas importadas. La nota "Parcial" quedó obsoleta: `b90013c`
  (fase 3, anterior a esta revisión del checklist) ya cableó el injerto real
  a `replay_page_ops`, que hoy materializa `PageOrigin::Imported` vía
  `pdf_manip::graft_pages` en vez de rechazarla (`core/pdf-save/src/bridge.rs`,
  función `replay_page_ops`, rama `PageOrigin::Imported`).
- [x] Mantener `PageId` como identidad estable e independiente del índice
  visual o de renderizado. La nota "Parcial" también quedó obsoleta: `34617d8`
  ya migró anotaciones, formularios, selección y edición de contenido en el
  shell Linux a resolver por `backend_index`/`render_index` en vez de
  `PageId.0`. Verificado por búsqueda: no quedan usos de `.id.0` como índice
  en `core/pdf-edit` ni en `apps/linux-gtk` (los únicos `.id.0` restantes son
  de `ContentItemId`/`FieldId`, identidades distintas).
- [x] Asignar identificadores únicos a cada PDF importado.
- [x] Registrar cada fuente importada una sola vez en el respaldo de la sesión.
- [x] Añadir un comando atómico para insertar todas las páginas seleccionadas
  de un PDF.
- [x] Hacer que deshacer y rehacer una importación requiera un único paso.
- [x] Añadir un comando atómico para mover un tramo contiguo de páginas.
- [x] Hacer que mover un bloque requiera un único paso de deshacer.
- [x] Añadir validación para impedir identificadores duplicados, rangos inválidos
  y órdenes que no sean permutaciones del estado actual. Los rangos inválidos
  se rechazan en los comandos de página — `Command::apply` acota `InsertPage`,
  `RemovePage`, `MovePage`, `ImportPages`, `RemoveImportedPages` y `MovePages`,
  y devuelve `false` sin mutar en lugar de paniquear. `MovePages` solo rota un
  slice validado de las páginas existentes, así que su resultado siempre es
  una permutación exacta. `InsertPage` e `ImportPages` rechazan `PageId`
  repetidos en el documento o dentro del lote. `ImportedSources` (en
  `pdf-save`) rechaza un `ImportedDocumentId` repetido en el registro de
  fuentes antes de injertar cualquier página.
- [x] Centralizar en el núcleo la clasificación de comandos estructurales de
  página.
- [x] Probar aplicación, inversión, deshacer y rehacer de los nuevos comandos.

### Progreso del modelo

- 2026-09-08: `pdf-document` incorpora `PageOrigin::{Base, Blank, Imported}` e
  `ImportedDocumentId`. La procedencia conserva índices de fuente separados de
  `PageId`; los bytes y objetos PDF siguen fuera del modelo puro.
- 2026-09-08: `pdf-save` registra como `Base` las páginas abiertas y rechaza
  páginas `Imported` hasta que exista el injerto real, evitando convertirlas
  silenciosamente en páginas en blanco.
- 2026-09-08: `Command::ImportPages` inserta un lote completo y conserva las
  páginas en su inversa `RemoveImportedPages`; undo y redo mueven todo el lote
  en una sola entrada. `Command::is_page_structure_edit` centraliza la
  clasificación que consume el shell Linux.
- 2026-09-08: `Command::MovePages { from, count, to }` mueve un tramo contiguo
  como una sola entrada de historial. `to` es la posición inicial final del
  tramo, por lo que la inversa intercambia `from` y `to`. La aplicación valida
  lote vacío, overflow y ambos extremos antes de rotar el slice en memoria;
  conserva identidad, procedencia, orden interno y la permutación exacta sin
  clonar páginas. El cableado de este comando al futuro arrastre de tarjetas de
  documento sigue perteneciendo a la fase 9.
- 2026-09-08: los comandos que añaden páginas validan la identidad antes de
  mutar. `InsertPage` rechaza un `PageId` ya presente e `ImportPages` rechaza
  tanto colisiones con el documento como duplicados internos del lote. La
  validación conjunta preserva la unicidad que necesitan `render_index` y los
  mapas de guardado, y un rechazo conserva documento, undo y redo mediante el
  contrato existente de `EditLog`.
- 2026-09-08: `ImportedSources::new` (en `pdf-save`) sigue aceptando un
  `ImportedDocumentId` repetido en construcción — sigue siendo un slice
  prestado, no un mapa — pero `replay_page_ops` ahora lo rechaza antes de
  injertar la primera página, con el mismo chequeo "arriba del todo" que ya
  usa para una fuente ausente. Sin esto, dos entradas con el mismo id
  resolverían en silencio la segunda fuente bajo el id de la primera, porque
  `ImportedSources::get` devuelve la primera coincidencia. Cierra el último
  punto abierto en [[pageid-uniqueness-insert-import]].
- Verificación (2026-09-08, unicidad de `ImportedDocumentId`): `cargo test -p
  pdf-save --locked` (24 aprobadas + 8 del roundtrip de importación, 0
  fallos); `cargo test --workspace --locked -- --skip gtk_ui_` (0 fallos);
  `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets
  --locked -- -D warnings`; `python scripts/check_maintainability.py` (99
  avisos, línea base sin cambios). No se ejecutó runtime GTK ni smoke de
  Linux: este cambio solo toca `pdf-save` y no tiene caller de plataforma
  todavía.
- Verificación (2026-09-08, unicidad de `PageId`): `cargo test -p pdf-document
  --locked` (117 aprobadas, 0 fallos); `cargo test --workspace --locked --
  --skip gtk_ui_` (0 fallos); `cargo fmt --all -- --check`; `cargo clippy
  --workspace --all-targets --locked -- -D warnings`; `git diff --check`;
  `python scripts/check_maintainability.py` (99 avisos, línea base sin cambios).
  No se ejecutó runtime GTK ni smoke de Linux: esta entrega solo cambia el
  modelo puro y no añade callers de plataforma.
- Verificación (2026-09-08, movimiento atómico de tramos): `cargo test -p
  pdf-document --locked` (114 aprobadas, 0 fallos); `cargo test --workspace
  --locked -- --skip gtk_ui_` (0 fallos); `cargo fmt --all -- --check`; `cargo
  clippy --workspace --all-targets --locked -- -D warnings`; `python
  scripts/check_maintainability.py` (99 avisos, línea base sin cambios). No se
  ejecutó runtime GTK ni smoke de Linux: este paso solo modifica el modelo puro
  y todavía no tiene caller de plataforma.
- 2026-09-08: los índices de página dejan de paniquear. `Command::apply`
  valida `InsertPage`, `RemovePage` y `MovePage` y devuelve `false` en vez de
  dejar que `Vec` aborte, igual que ya hacían las variantes por lote. En el
  límite FFI, `FfiEditCommand::InsertBlankPage` no acotaba su índice —
  a diferencia de `RemovePage`, tres líneas más abajo — así que un índice
  fuera de rango llegaba a `Vec::insert` y paniqueaba: a través de UniFFI eso
  es un abort no capturable que se lleva el proceso anfitrión y el documento
  sin guardar. Ahora devuelve `PageIndexOutOfBounds`. `MovePage` acota sus
  dos puntas: `from` se quita antes de insertar en `to`, de modo que un `to`
  sin validar paniquea contra un vector ya acortado. Las eliminaciones de una
  sola página se acotan por rango en lugar de comparar la `Page` que llevan
  (como sí hace `RemoveImportedPages`), porque `RotatePage` muta la `Page` en
  el sitio dentro de `document.pages` y un contraste por igualdad rechazaría
  eliminaciones válidas.
- 2026-09-08: el `bool` de `EditLog::apply` deja de descartarse. Los dos
  ayudantes `apply_command` (shell Linux y `pdf-ffi`) lo propagan; el shell lo
  convierte en mensaje de estado y el FFI en `UnsupportedOperation`. Sin esto,
  un lote rechazado se reportaba como éxito y dejaba la línea de estado y la
  pila de deshacer describiendo una edición que nunca ocurrió. No se marcó
  `EditLog::apply` como `#[must_use]`: unas 70 llamadas de prueba lo ignoran
  como sentencia y `-D warnings` tumbaría la verificación entera.
- Verificación (2026-09-08, índices de página y propagación del rechazo):
  `cargo test --workspace --locked` (819 aprobadas, 0 fallos); `cargo test -p
  pdf-document --locked` (106 aprobadas); `cargo test -p pdf-ffi --locked`
  (61 aprobadas); `cargo fmt --all -- --check`; `cargo clippy --workspace
  --all-targets --locked -- -D warnings`; `python
  scripts/check_maintainability.py` (99 avisos, línea base sin cambios).
  `linux-gtk` no compila en Windows, así que el cambio en `organize.rs` solo
  pasó por el analizador de `cargo fmt`: su verificación real es el CI de
  Linux.
- Verificación (2026-09-08, comandos de importación): `cargo test -p
  pdf-document --locked` (102 aprobadas); `cargo test --workspace --locked --
  --skip gtk_ui_` (0 fallos); `cargo fmt --all -- --check`; `cargo clippy
  --workspace --all-targets --locked -- -D warnings`; `python
  scripts/check_maintainability.py` (99 avisos, línea base sin cambios). En
  WSL2/Ubuntu, `cargo test -p linux-gtk --locked -- --skip
  package_smoke::tests::renders_the_embedded_sample_to_a_nonempty_receipt`
  (347 aprobadas, 1 filtrada). El package smoke completo no se verificó: esta
  copia no contiene `libpdfium.so` para Linux.
- Verificación: `cargo fmt --all -- --check`; `cargo test -p pdf-document`
  (86 aprobadas); `cargo test -p pdf-save` (119 aprobadas, 4 ignoradas);
  `cargo clippy -p pdf-document -p pdf-save --all-targets -- -D warnings`.
- 2026-09-08: `pdf-save` gana `ImportedSourceRegistry`
  (`core/pdf-save/src/imported_sources.rs`), el registro de sesión que faltaba
  entre `ImportedSources` (prestado, vive solo durante un guardado) y el shell
  Linux (que hoy sigue pasando `ImportedSources::none()` en sus tres sitios de
  guardado — cablearlo es fase 7/8, no este paso). `register()` asigna el
  siguiente `ImportedDocumentId` libre (máximo actual + 1, mismo patrón que
  `next_form_field_id` del shell Linux) y añade la fuente una sola vez;
  `pairs()` expone `(id, &LopdfDocument)` para que el llamador construya un
  `ImportedSources` en el momento de guardar, igual que ya documenta
  `ImportedSources::new`.
- Decisión de arquitectura: el registro vive en `pdf-save` (núcleo, sin
  dependencias de GTK) y no en `DocumentSession` del shell Linux, para que la
  asignación de ids y el almacenamiento de fuentes sean testeables en
  cualquier plataforma — igual que los pasos anteriores de esta fase, que
  quedaron en el núcleo puro antes de tener un caller en Linux. `DocumentSession`
  podrá poseer una instancia de `ImportedSourceRegistry` cuando la fase 8 añada
  el selector de archivos; hasta entonces no hay caller que lo use, y no se ha
  tocado ningún sitio de guardado del shell.
- Verificación (2026-09-08, registro de fuentes importadas): `cargo test -p
  pdf-save --locked` (24 + 8 del roundtrip + 4 nuevas del registro, 0 fallos);
  `cargo test --workspace --locked -- --skip gtk_ui_` (0 fallos); `cargo fmt
  --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D
  warnings`; `python scripts/check_maintainability.py` (99 avisos, línea base
  sin cambios). No se ejecutó runtime GTK ni smoke de Linux: este cambio solo
  añade un tipo nuevo en `pdf-save` y no tiene caller de plataforma todavía.

## 2. Derivación de bloques

- [x] Derivar los bloques recorriendo `Document.pages` de izquierda a derecha.
- [x] Agrupar únicamente páginas contiguas procedentes del mismo PDF.
- [x] Mantener el orden interno exacto de cada tramo.
- [x] Crear un bloque nuevo cuando reaparezca una fuente después de páginas de
  otra fuente.
- [x] Identificar cada bloque mediante datos estables, no mediante su posición
  visual.
- [x] Etiquetar tramos divididos como `Part 1`, `Part 2`, etc.
- [x] Recalcular los bloques después de importar, mover, borrar, deshacer o
  rehacer.
- [x] Probar secuencias intercaladas como `A1, A2, B1, A3, B2`.
- [x] Verificar que derivar bloques nunca muta el documento.

### Progreso de la derivación de bloques

- 2026-09-08: `pdf-document` gana `blocks.rs` (`derive_blocks`, `Block`,
  `BlockSource`). Es una función pura sobre `&Document`: recorre
  `Document.pages` una vez y arranca un bloque nuevo cada vez que cambia la
  fuente respecto de la página anterior — incluida una fuente que ya apareció
  antes, que es justo lo que la vuelve "no contigua" y hay que partir en
  bloques separados.
- `BlockSource` es deliberadamente más angosto que `PageOrigin`: descarta
  `page_index` porque dos páginas de la misma fuente en índices distintos
  siguen siendo el mismo bloque. Tiene tres variantes — `Base`, `Blank`,
  `Imported(ImportedDocumentId)` — así que las páginas en blanco también se
  agrupan en bloques contiguos aunque no vengan de un PDF.
- Sin caché: no hay invalidación que mantener. Cada llamada recorre
  `Document.pages` desde cero, así que recalcular tras importar, mover,
  borrar, deshacer o rehacer es automático por construcción — no hace falta
  ningún paso adicional del llamador más que volver a invocar la función.
  Verificado con un test que aplica `ImportPages`, `MovePages`, `undo` y
  `redo` en secuencia y deriva bloques después de cada paso.
- Identidad estable: cada `Block` lleva `anchor`, el `PageId` de su primera
  página — nunca la posición en el `Vec` que devuelve `derive_blocks`, que
  cambia en cada recálculo.
- Etiquetado de partes: `part: Option<u32>` es 1-based y solo se asigna
  cuando una fuente aparece en más de un bloque. El núcleo no fija el string
  "Part N" — solo el número — para que la presentación quede del lado de la
  UI, igual que el resto de los datos de este crate no conoce GTK.
- Verificación: `cargo test -p pdf-document --locked` (126 aprobadas, 0
  fallos); `cargo test --workspace --locked -- --skip gtk_ui_` (0 fallos);
  `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets
  --locked -- -D warnings`; `python scripts/check_maintainability.py` (99
  avisos, línea base sin cambios). No se ejecutó runtime GTK ni smoke de
  Linux: este cambio solo añade un módulo puro en `pdf-document` y todavía
  no tiene caller de plataforma — cablearlo a la vista "Documents" es la
  fase 9.

## 3. Importación PDF

- [x] Implementar una operación de injerto de páginas en `pdf-manip`.
- [x] Copiar y remapear el grafo de objetos alcanzable desde cada página.
- [x] Materializar los atributos heredados necesarios antes de cambiar el
  padre de una página.
- [x] Conservar streams de contenido, fuentes, imágenes, XObjects, espacios de
  color y patrones.
- [x] Conservar cajas de página, rotación y recursos heredados.
- [x] Conservar enlaces, acciones y anotaciones asociados a las páginas.
- [x] Evitar colisiones entre identificadores de objetos de distintos PDFs.
- [x] Excluir de la copia el catálogo, trailer, cifrado y metadatos globales de
  la fuente cuando no deban gobernar el documento resultante.
- [x] Devolver el mapa de objetos necesario para resolver las páginas
  importadas después del injerto.
- [x] Hacer que una entrada malformada falle sin modificar parcialmente el
  destino.

### Progreso del injerto

- 2026-09-08: `pdf_manip::graft_pages(destino, índice, fuente, páginas)` copia
  páginas reales de un PDF a otro y devuelve el documento resultante. No
  rasteriza nada.
- No es `merge` y no podía serlo: `merge` construye un documento nuevo a partir
  de varias fuentes, y aquí el destino ya está abierto — su catálogo, su
  política de cifrado, su `/Info`, sus identificadores de objeto y sus páginas
  tienen que sobrevivir intactos, porque hay un `EditLog` entero apuntando a
  ellos. El injerto clona el destino tal cual y solo trae lo seleccionado.
- Colisiones de identificadores: se resuelven una sola vez renumerando un clon
  de la fuente por encima del `max_id` del destino, en vez de remapear
  referencia por referencia.
- Atributos heredados (`/Resources`, `/MediaBox`, `/CropBox`, `/Rotate`) se
  materializan sobre la página antes de reparentarla, porque el nodo `/Pages`
  del que los heredaba se queda atrás. El aplanado ocurre ANTES del recorrido:
  materializar `/Resources` mete una referencia nueva en la página que el
  recorrido después tiene que seguir.
- El recorrido sigue referencias transitivamente e incluye los diccionarios de
  los streams, no solo los diccionarios planos: una imagen indexada necesita su
  tabla de consulta y su `/SMask`, y ambos cuelgan del diccionario del stream.
  Hay un test que camina esa cadena entera.
- Referencias a páginas NO seleccionadas: el recorrido se detiene ahí. Seguirlas
  arrastraría casi toda la fuente detrás de un solo enlace. La referencia queda
  apuntando a un objeto ausente, que PDF 32000-1:2008 §7.3.10 define como una
  referencia a null — el enlace queda inerte, no corrupto. Remapear destinos
  importados y avisar de los que no lo están es trabajo de la fase 4.
- Resolución posterior al injerto: las páginas injertadas caen contiguas en
  `índice..índice + páginas.len()` y en el orden pedido, así que el llamador las
  resuelve por posición igual que ya hace `bridge::page_object_ids`. No se
  devuelve un mapa de objetos aparte porque no haría falta ninguno.
- Se rechaza la misma página fuente dos veces
  (`ManipError::DuplicatePageSelection`): cada página injertada conserva el
  identificador de objeto de su fuente para que las referencias entre páginas
  importadas sigan siendo válidas, y un mismo objeto no puede ocupar dos
  posiciones del árbol de páginas.
- Atomicidad: la validación entera ocurre antes de copiar el primer objeto, y
  el destino se recibe por referencia, así que un injerto rechazado no puede
  dejar un documento a medio importar.
- 2026-09-08 (segunda entrega): el injerto ya es alcanzable desde un guardado.
  `SaveInput` lleva `imported_sources: ImportedSources<'a>` y
  `bridge::replay_page_ops` materializa las páginas `Imported` llamando a
  `graft_pages`.
- Verificación: `cargo test -p pdf-manip` (15 tests de injerto, incluido un
  round trip real de serializar y volver a abrir); `cargo fmt --all -- --check`;
  `cargo clippy --workspace --all-targets --locked -- -D warnings`;
  `python3 scripts/check_maintainability.py` (99 avisos, línea base sin
  cambios).

## 4. Estructuras de documento

- [x] Definir y probar la política para formularios AcroForm importados.
- [ ] Resolver colisiones de nombres de campos sin fusionarlos silenciosamente.
- [ ] Conservar widgets y apariencias cuando se acepten formularios.
- [x] Definir y probar la política para marcadores y destinos con nombre.
- [x] Remapear destinos que apunten a páginas importadas.
- [x] Detectar destinos que apunten a páginas no importadas.
- [x] Definir la política para capas opcionales y estructura etiquetada.
- [x] Rechazar con un mensaje claro cualquier estructura todavía no soportada
  que pudiera perder información.
- [x] No ofrecer una importación aparentemente correcta si existe pérdida de
  datos conocida. Linux muestra el `GraftReport` completo antes de confirmar;
  las estructuras que producirían salida incorrecta siguen rechazándose.

### Progreso de la política de formularios

- 2026-09-08: decisión de alcance — `graft_pages` rechaza el injerto en vez de
  fusionar campos. Fusionar un widget importado en el `/AcroForm` del destino
  exige una política de colisión de nombres y de conservación de apariencias
  que todavía no existe; ofrecer una importación que se ve correcta pero
  arrastra un widget huérfano (sin campo en `/AcroForm`, o con un `/T`
  duplicado) sería exactamente la pérdida de datos silenciosa que la última
  regla de esta sección prohíbe. Los ítems 2 y 3 quedan pendientes: son el
  trabajo de implementar la fusión, no aplicable mientras la política sea
  "rechazar".
- La comprobación mira solo las páginas seleccionadas, no el `/AcroForm` de la
  fuente entera: un PDF fuente puede tener un formulario en páginas que no se
  importan y el injerto de las demás páginas debe seguir funcionando. Se
  detecta por página (`/Annots` con un `/Subtype /Widget`) en vez de por
  `/AcroForm /Fields`, porque un campo solo afecta a una página importada a
  través del widget que cuelga de su propio `/Annots`.
- Nuevo `ManipError::SourceHasFormFields(usize)` (índice 0-based de la página
  en la selección pedida), igual que `InvalidPageIndex`. `pdf-ffi` ya cae en
  su rama `other => Internal { detail }` sin cambios porque `ManipError` es
  `#[non_exhaustive]`.
- Verificación: `cargo test -p pdf-manip` (19 tests, incluidos los 2 nuevos de
  `tests/graft_form_fields.rs`); `cargo fmt --all -- --check`;
  `cargo clippy -p pdf-manip -p pdf-save -p pdf-ffi --all-targets --locked --
  -D warnings`; `python3 scripts/check_maintainability.py` (99 avisos, línea
  base sin cambios — los tests nuevos se separaron en su propio archivo para
  no empujar `tests/graft.rs` sobre el umbral de 350 líneas).

### Progreso de destinos, marcadores, capas y estructura etiquetada

- 2026-09-08 (decisión de contrato): el injerto deja de devolver solo un
  documento. `graft_pages` devuelve `GraftOutcome { document, report }` y hay
  un `graft_report(fuente, páginas)` puro que responde lo mismo **sin importar
  nada** — la puerta que el flujo de importación consulta en el momento de
  seleccionar, cuando el usuario todavía puede cambiar de opinión. La
  alternativa era convertir cada estructura no soportada en un `ManipError`
  como se hizo con AcroForm; se descartó porque rechazaría casi todo PDF de
  oficina (prácticamente todos vienen etiquetados) y dejaría la importación
  inservible.
- Regla: *ninguna importación con pérdida puede parecerse a una sin pérdida*.
  Cada estructura del catálogo cae en una de dos respuestas, nunca en una
  tercera. Se **rechaza** lo que saldría MAL (widget AcroForm; contenido
  opcional, porque su configuración `/OCProperties` se queda atrás y una capa
  que el autor apagó puede volver visible — eso no es pérdida, es salida
  incorrecta). Se **avisa** lo que sale íntegro pero más pobre (destinos
  colgantes, marcadores, estructura etiquetada).
- Destinos explícitos: ya funcionaban gratis y ahora hay test que lo fija. El
  injerto renumera la fuente entera una vez y conserva el id de cada página
  injertada, así que `/Dest [12 0 R /XYZ …]` sigue cayendo en la misma página.
- Destinos con nombre: NO funcionaban. El mapa nombre→página vive en el
  catálogo (`/Names /Dests` o el `/Dests` de PDF 1.1) y el catálogo es
  justamente lo que no se copia. Se resuelven contra la fuente mientras la
  fuente está a mano y se escribe en el enlace importado el destino explícito
  que significaban, conservando los parámetros de vista (`/XYZ`, `/FitH`) en
  vez de inventar uno. Se buscan las dos formas, árbol de nombres y
  diccionario heredado.
- El árbol de nombres se recorre entero ignorando `/Limits`: es una pista de
  orden que un productor puede escribir mal, y confiar en una pista errónea
  daría un destino por irresoluble en silencio.
- Marcadores: el `/Outlines` de la fuente no se importa — cuelga del catálogo
  y está ordenado contra el orden de páginas de la fuente. Se cuentan solo las
  entradas que apuntaban DENTRO de la selección; contar las demás sería dar
  falsas alarmas con cualquier PDF que tenga índice.
- Estructura etiquetada: se avisa, no se rechaza. Se detecta con
  `/StructParents` en la página MÁS `/StructTreeRoot` en el catálogo: uno sin
  el otro no apunta a nada y no hay nada que perder.
- Anotaciones escritas en línea dentro de `/Annots` (sin objeto propio) no se
  pueden reescribir; se leen para avisar, así que un destino con nombre en una
  de ellas se reporta como caído en vez de viajar muerto.
- El guardado descarta el reporte a propósito (`bridge.rs`): un guardado es la
  reproducción de una importación que el usuario ya eligió, y quien enseña el
  costo es `graft_report` en el momento de seleccionar. Lo que el guardado no
  descarta es un rechazo — eso sigue siendo un error que corta el guardado.
- Reparto de módulos en `pdf-manip`: `page_graph.rs` (cómo una página se
  desprende de su árbol y qué alcanza — movido tal cual desde `graft.rs`, sin
  cambio de comportamiento), `destinations.rs` (qué significa un destino),
  `links.rs` (quién lleva uno: anotaciones y marcadores) y `report.rs` (la
  política y el reporte). `graft.rs` quedó en 175 líneas.
- Verificación: `cargo test --workspace --locked` (865 tests, 0 fallos;
  incluidos `tests/graft_destinations.rs` con 8 y `tests/graft_structures.rs`
  con 7); `cargo fmt --all -- --check`; `cargo clippy --workspace
  --all-targets --locked -- -D warnings`;
  `python3 scripts/check_maintainability.py` (99 avisos, línea base sin
  cambios — `destinations.rs` se partió en dos y `tests/graft.rs` recuperó un
  helper para no cruzar el umbral de 350 líneas). Los dos tests del remapeo se
  comprobaron por mutación: desactivando la reescritura, fallan.

## 5. Seguridad y firmas

- [x] Añadir una comprobación específica del permiso PDF de ensamblado de
  documentos.
- [x] Comprobar permisos tanto en el documento principal como en cada fuente.
- [x] Solicitar de forma independiente la contraseña de cada PDF protegido.
- [x] Mantener las credenciales fuera del modelo de dominio, logs y mensajes de
  error.
- [x] Verificar antes de editar que un documento principal cifrado puede
  reescribirse correctamente.
- [x] Mantener la política de cifrado del documento principal al guardar.
- [x] Detectar firmas en el documento principal y en las fuentes activas.
- [x] Explicar que una combinación o reordenación invalida criptográficamente
  las firmas existentes.
- [x] Exigir confirmación explícita antes de guardar un resultado que invalide
  firmas.
- [x] Conservar objetos y apariencias de firma solo cuando la política definida
  lo permita.

### Progreso del permiso de ensamblado

- 2026-09-08: `pdf_manip::document_assembly_is_allowed` es la cuarta política
  de `core/pdf-manip/src/security.rs`, el único sitio donde se interpreta un
  bit de `/P`. Lee `/P` bit 11 (`1 << 10`, `lopdf::Permissions::ASSEMBLABLE`)
  **o** el bit 4 de modificación general: la tabla 22 de PDF 1.7 define el bit
  11 como permitir el ensamblado *"even if bit 4 is clear"*, o sea que el bit
  4 ya lo lleva consigo. Exigir solo el bit 11 inventaría una restricción que
  ningún documento con manejador de revisión 2 —donde el bit 11 no significa
  nada— llegó a declarar.
- El hueco que cierra era real y silencioso: `RotatePage`, `InsertBlankPage` y
  `RemovePage` cruzaban `pdf-ffi::apply_edit` sin permiso alguno.
  `is_annotation_command` los excluye explícitamente y `is_content_command`
  tampoco los reclama, así que no los comprobaba nadie. Un documento que
  prohíbe el ensamblado podía ser repaginado igual.
- `Command::is_document_assembly_edit` (en `pdf-document`) es un predicado
  aparte de `is_page_structure_edit` y no una ampliación suya: se diferencian
  en exactamente una variante, `RotatePage`, que no cambia ni la pertenencia
  ni el orden de las páginas —así que no es un cambio de estructura— pero que
  la tabla 22 nombra literalmente ("insert, **rotate**, or delete pages").
- En el shell GTK4 la puerta va en `organize::command`, el embudo por el que
  ya pasa toda operación de la pantalla, junto al chequeo de edición de
  contenido que ya había. Un documento puede conceder uno de los dos bits y
  negar el otro, de modo que preguntar solo por el primero repaginaría un
  archivo que lo prohíbe. `PageAssemblyAccess` copia la forma de `TextAccess`
  (tres estados, `Unreadable` incluido) y no la de `ContentEditAccess`: es una
  pregunta de permiso pura, y la ausencia del modelo editable ya la reporta
  la propia pantalla con sus palabras.
- Fuentes: la comprobación por archivo vive en
  `ImportedSourceRegistry::register`, que ahora recibe el `SecurityContext` de
  *esa* fuente y devuelve `Result`. Es el único punto por el que una fuente
  entra en la sesión, así que rechazar ahí impide que un PDF no importable
  llegue a la lista de páginas y se convierta en un guardado que falla más
  tarde. El bit que la gobierna es el 5 —el que lee
  `text_extraction_is_allowed`—, cuyo texto en la tabla 22 es "copy or
  otherwise extract text **and graphics** from the document": levantar el
  contenido de una página hacia otro archivo es exactamente eso. El nombre
  dice *text* porque la extracción de texto fue su primer llamador, no porque
  el bit sea más estrecho que la operación. Nuevo
  `SaveError::SourceForbidsImport`.
- Dos pruebas de `pdf-ffi/tests/smoke.rs` pasaron a abrir con la contraseña de
  propietario: el corpus `rc4_128_user_and_owner.pdf` concede impresión y
  copia y nada más, así que una apertura con credencial de usuario ya no puede
  ensamblarlo. Su asunto es la indexación de páginas y la imposibilidad de
  reescribir con una sola contraseña, no los permisos; el rechazo en sí tiene
  su propia prueba nueva.
- Verificación (2026-09-08, permiso de ensamblado): `cargo test --workspace
  --locked -- --skip gtk_ui_` (878 aprobadas, 0 fallos); `cargo test -p
  pdf-document -p pdf-manip -p pdf-ffi -p pdf-save --locked` (0 fallos);
  `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets
  --locked -- -D warnings`; `python scripts/check_maintainability.py` (99
  avisos, línea base sin cambios). `linux-gtk` no compila en Windows: los
  cambios de `organize.rs`, `state.rs`, `document.rs` y su prueba
  `gtk_ui_refused_and_failed_commands_leave_cards_and_history_untouched` solo
  pasaron por el analizador de `cargo fmt`; su verificación real es el CI de
  Linux.

### Progreso de credenciales, reescritura y política de cifrado

- 2026-09-09 (decisión de contrato): la pregunta *"¿se puede reescribir este
  documento?"* se separa del codificador que lo reescribe. `pdf-save` gana
  `rewrite.rs` con `RewriteBlocker` y `full_rewrite_blocker(security)`, y
  `security::build_encryption_state` pasó a consultarlo en vez de repetir la
  regla. Son dos responsabilidades y dos momentos —una se pregunta mientras el
  usuario todavía elige qué hacer, la otra en el último instante del guardado—
  pero una sola función, de modo que no pueden discrepar. Hay un test que fija
  exactamente eso: el codificador rechaza con las mismas palabras que reporta
  el bloqueo.
- El hueco era el mismo que ya se había cerrado para la edición de contenido,
  pero para las páginas: un documento cifrado abierto con una sola de sus dos
  contraseñas aceptaba mover o borrar páginas y recién fallaba al guardar, con
  el reordenamiento ya hecho. Cualquier cambio estructural fuerza el escritor
  de reescritura completa (`has_structural_page_changes`), y una reescritura no
  puede reconstruir la contraseña que falta: la contraseña de propietario de un
  PDF no se deriva de la de usuario ni al revés.
- La frontera importa y no es la del permiso: `RotatePage` es una operación de
  ensamblado (tabla 22 la nombra) pero **no** un cambio de estructura —no
  cambia ni la pertenencia ni el orden—, así que sigue en el escritor
  incremental, que reencripta desde el estado que lopdf ya retiene y no
  necesita ninguna contraseña nuestra. Por eso `pdf-ffi` gana
  `is_page_structure_command`, gemelo de `Command::is_page_structure_edit`, y
  la puerta de reescritura pregunta por ese predicado y no por el de
  ensamblado. Preguntar por el ancho rechazaría una rotación que se guarda
  perfectamente.
- Lo mismo vale para el resto del embudo del shell: rellenar formularios,
  editar metadatos y firmar también son incrementales, así que la comprobación
  no se plegó dentro de `content_edit_refusal` —que esos tres consultan— sino
  que vive en `Viewer::full_rewrite_refusal` y la piden solo los embudos que
  fuerzan la reescritura. Hoy eso es `organize::command` (mover y borrar
  páginas).
- La copia local que `pdf-ffi` tenía de la regla (`content_edit_could_be_saved`)
  miraba solo la mitad de las contraseñas, así que dejaba pasar un documento
  AES-256 hasta un guardado que después lo rechazaba. Al delegar en
  `full_rewrite_blocker` esa segunda condición —el manejador sin
  implementación de reencriptado— quedó cubierta también.
- Credenciales fuera de los mensajes: `EncryptionCredentials` ya tenía un
  `Debug` escrito a mano que redacta ambas contraseñas, pero nada impedía que
  un `#[derive(Debug)]` lo reemplazara y filtrara en silencio; ahora hay
  pruebas que lo fijan, incluida la del contenedor (`SecurityContext` deriva
  `Debug`, así que solo es tan seguro como el campo). Del lado de los
  mensajes, `RewriteBlocker::reason` es `&'static str` por tipo: un motivo
  nunca se formatea a partir del `SecurityContext` que describe, que es lo
  único que podría arrastrar una contraseña hasta un log o un informe de
  error.
- Política de cifrado al guardar: `apply_encryption_for_full_rewrite` ya la
  reaplicaba, y la prueba existente fijaba que las dos contraseñas siguen
  abriendo el resultado. Faltaba la otra mitad —el manejador y el `/P`—, que
  ahora se comprueba releyendo el archivo reescrito con
  `read_security_context_from_bytes` después de mover una página. Una
  reescritura que reencriptara con otro manejador, o con permisos ensanchados,
  pasaba la prueba anterior y entregaba igual un documento cuyas restricciones
  ya no dicen lo que su autor escribió.
- Observación adjunta, fuera del alcance de este lote: en el shell GTK4 la
  edición de *contenido* todavía no hace esta comprobación (sí la hace
  `pdf-ffi`, o sea todos los demás shells). Es un hueco del lote 21, no de la
  ruta de ensamblado, y se cierra donde se decida que va la puerta de
  `content_edit`.
- Verificación (2026-09-09, reescritura cifrada y credenciales): `cargo build
  --workspace --locked`; `cargo test --workspace --locked -- --skip gtk_ui_`
  (891 aprobadas, 0 fallos); `cargo fmt --all -- --check`; `cargo clippy
  --workspace --all-targets --locked -- -D warnings`; `git diff --check`;
  `python scripts/check_maintainability.py` (99 avisos, línea base sin
  cambios — `pdf-save/src/security.rs` se partió en `rewrite.rs` y el test de
  rechazos de Organizar se separó en
  `organize/tests/refusals.rs` para no cruzar el umbral de 350 líneas).
  `linux-gtk` no compila en Windows: los cambios de `state.rs`, `organize.rs`
  y el nuevo `organize/tests/refusals.rs` solo pasaron por el analizador de
  `cargo fmt`; su verificación real es el CI de Linux.

### Progreso de firmas

- 2026-09-09 (decisión de alcance): la detección de firmas pasa a vivir en
  `pdf-manip` (`core/pdf-manip/src/signatures.rs`), no en `pdf-save`. Es una
  pregunta sobre el grafo de objetos PDF —el crate cuyo trabajo declarado es
  justamente ese— y hay dos lados que tienen que responderla igual: el
  destino, en el momento de guardar, y cada fuente, en el momento de
  seleccionar. `pdf_save::has_signatures` se retiró y sus llamadores usan
  `pdf_manip::document_has_signatures`; había una sola definición y ahora
  sigue habiendo una sola, pero alcanzable desde ambos lados.
- Son **dos preguntas con dos alcances** y no se pueden confundir. El
  `/ByteRange` de una firma cubre el archivo entero, así que "¿este archivo
  está firmado?" es un hecho del documento. "¿esta página lleva el widget de
  una firma?" es un hecho de la página, y es el único que un injerto puede
  arrastrar.
- Política de la fuente (ítem 10): una página seleccionada que lleva un widget
  de firma se **rechaza** — `ManipError::SourceHasSignature`. El widget es la
  apariencia ("firmado por …", el nombre, la fecha, el sello); el campo, su
  diccionario `/V` y el rango de bytes que ese diccionario cubre se quedan en
  la fuente. Copiar solo el widget pondría en el destino un bloque de firma
  que da fe de un archivo que nadie puede contrastar: no es una importación
  más pobre, es una con forma de falsificación, y es lo peor que podía hacer
  la regla de la sección 4.
- Se comprueba **antes** que el widget genérico de AcroForm, que también
  coincidiría: una firma es un campo de formulario, y responder "esta página
  tiene campos de formulario" escondería lo único que importaba. El error
  específico gana, y hay una prueba que fija que el mensaje no dice "form
  fields".
- El `/FT` puede estar en el propio widget (el caso fusionado, el habitual) o
  en un ancestro por `/Parent`, así que se recorre la cadena con un tope de
  profundidad —una cadena malformada que se apunta a sí misma no puede girar—
  y un `/V` que resuelve a un `/Type /Sig` también cuenta: es la firma misma
  colgada del campo, y un productor que omita `/FT` en algún eslabón no debe
  colar una firma real.
- Fuente firmada, páginas limpias (ítem 7): se **avisa**, no se rechaza —
  `GraftWarning::SourceSignaturesNotImported`. Es el único aviso que no nombra
  una página, y a propósito: la firma cubre el archivo entero, así que es un
  hecho de la fuente e igual de cierto para cualquier página que se saque de
  ella. El archivo original queda intacto y sigue verificando; lo que el
  usuario necesita saber es que la copia que está armando no hereda eso.
- Ítems 8 y 9 (destino) ya estaban implementados desde el lote 21 —
  `pdf_save::will_invalidate_signatures`, `SignatureAcknowledgement`,
  `SaveError::SignaturesWouldBeInvalidated` y el diálogo
  `document::confirm_signature_loss` del shell GTK4, que explica que la firma
  no se elimina sino que deja de coincidir y que guardar a otro archivo
  conserva una copia que verifica. Lo que faltaba era la **prueba por la ruta
  de esta entrega**: toda la cobertura existente pasaba por ediciones de
  contenido. Reordenar llega al mismo escritor de reescritura completa por
  otro camino (`has_structural_page_changes` en vez de `has_content_edits`),
  así que un estrechamiento de cualquiera de las dos condiciones habría
  pasado inadvertido.
- El refresco de previsualización sigue reconociendo la invalidación en
  silencio (`refresh_snapshot_and_reopen` y `pdf_ffi::refresh_preview`), y eso
  se mantiene: guarda en memoria, nunca toca el disco y el llamador conserva
  el `SaveBacking` original. El diálogo es para el guardado que sí escribe.
- Verificación (2026-09-09, firmas): `cargo build --workspace --locked`;
  `cargo test --workspace --locked -- --skip gtk_ui_` (906 aprobadas, 0
  fallos; incluidas las 6 nuevas de `pdf-manip/tests/graft_signatures.rs`, las
  12 de `signatures.rs` y las 2 de reordenamiento firmado en
  `pdf-save/tests/save_roundtrip.rs`); `cargo fmt --all -- --check`; `cargo
  clippy --workspace --all-targets --locked -- -D warnings`; `git diff
  --check`; `python scripts/check_maintainability.py` (99 avisos, línea base
  sin cambios — `rewrite_named_destinations` se movió de `report.rs` a
  `links.rs`, que es donde vive escribir un destino de vuelta, para que
  `report.rs` no cruzara el umbral de 350 líneas). `linux-gtk` no compila en
  Windows: el cambio de `sign/mod.rs` solo pasó por el analizador de `cargo
  fmt`; su verificación real es el CI de Linux.

## 6. Guardado y resolución de páginas

- [x] Extender `pdf-save` para distinguir páginas en blanco de páginas
  importadas.
- [x] Reconstruir el PDF siguiendo exactamente el orden de `Document.pages`.
- [x] Mantener el documento base original inmutable durante la edición.
- [x] Resolver cada página importada mediante su fuente y número de página.
- [x] Devolver un mapa final de `PageId` a objeto PDF después de materializar.
- [x] Resolver explícitamente `PageId` a índice de renderizado actual.
- [x] Eliminar los usos que asumen que `PageId.0` es un índice de PDFium.
- [x] Leer anotaciones existentes desde el documento materializado para no
  borrar anotaciones importadas al añadir otras nuevas.
- [x] Permitir edición de contenido sobre páginas importadas usando el respaldo
  materializado correcto.
- [x] Validar el resultado con PDFium antes de instalar la previsualización.
- [x] Probar guardar, cerrar y reabrir después de importar, mover y borrar.

### Progreso del registro de fuentes

- `ImportedSources` es un slice prestado de `(ImportedDocumentId,
  &LopdfDocument)`, no un mapa: un documento tiene un puñado de fuentes
  importadas, no miles, y prestar un slice deja que quien llama arme uno en el
  stack sin tener que poseer una colección solo para decir "ninguna".
  `ImportedSources::none()` es la respuesta correcta para casi todos los
  guardados.
- Hacía falta porque `pdf_document` es un modelo puro sin dependencias de E/S:
  una página `Imported` solo puede nombrar su fuente por identificador, así que
  alguien tiene que cruzar los bytes en el momento de guardar.
- La comprobación de fuentes ausentes ocurre ANTES de copiar el primer objeto.
  Un guardado que no puede materializar una de sus páginas no escribe nada, en
  vez de frenarse a mitad con las páginas anteriores ya injertadas.
- Gotcha de rotación: `pdf_manip::rotate_page` aplica un DELTA, y una página
  injertada llega con el `/Rotate` de su fuente ya puesto (una en blanco llega
  en cero). La rotación del modelo es absoluta, así que se calcula la
  diferencia contra lo que la página trae en vez de sumarle: sumar convertiría
  una página importada que ya estaba a 90 y se modela a 90 en una de 180. Hay
  dos tests que fijan esto, incluido el de volver a cero.
- Verificación: `cargo test --workspace --locked` (1154 aprobadas, 0 fallidas,
  suite GTK4 incluida, ejecutada en WSL2/Ubuntu); `cargo fmt --all -- --check`;
  `cargo clippy --workspace --all-targets --locked -- -D warnings`;
  `python3 scripts/check_maintainability.py` (99 avisos, línea base sin
  cambios).
- Pendiente para que el usuario pueda importar de verdad: nada en el shell crea
  todavía páginas `Imported` ni guarda los PDFs importados en la sesión, así
  que `document::save_snapshot_and_reopen` pasa `ImportedSources::none()`. Eso
  es la fase 1 (comandos de importación) y la fase 8 (selección de archivos en
  Linux).

### Progreso de resolución de páginas

- 2026-09-08: `pdf-document` expone `Document::render_index`, `page_id_at` y
  `page_at`. Resuelven `PageId` contra la posición lógica en `Document.pages`,
  que es el orden que `pdf-save` materializa y contra el que
  `bridge::page_object_ids` empareja el PDF escrito.
- Existen DOS posiciones por página y no son intercambiables: la lógica
  (índice en `Document.pages`) y la del backend (índice de página del PDF que
  PDFium tiene abierto en ese momento). Coinciden al abrir y después de cada
  guardar/reabrir; divergen mientras haya operaciones de página sin guardar,
  porque el handle sigue con el orden previo.
- El shell Linux depende hoy del contrato `PageId.0` == índice de PDFium del
  handle abierto, y lo hace a propósito: `organize::populate_grid` pide las
  miniaturas por `page.id.0` mientras el handle conserva el orden original, y
  `refresh_snapshot_and_reopen` preserva el modelo (y sus ids) al reabrir.
- Por eso el orden de trabajo es obligatorio: primero refrescar PDFium después
  de cada operación de página, y solo entonces migrar los usos de `PageId.0`
  al mapa. Migrarlos antes introduce un fallo nuevo en vez de arreglar uno,
  porque el lienzo seguiría mostrando el orden previo.
- El injerto de páginas importadas va después de ese refresco: una página
  injertada recibe un `PageId` nuevo que nunca fue índice de nada, y ahí el
  contrato actual deja de sostenerse.
- Verificación (2026-09-08, primera entrega): `cargo fmt --all -- --check`;
  `cargo test --workspace --locked -- --skip gtk_ui_` (0 fallos);
  `cargo clippy --workspace --all-targets --locked -- -D warnings`. La suite
  GTK4 no se ejecutó en esa entrega: `linux-gtk` está compilado bajo
  `cfg(target_os = "linux")` y aquella verificación corrió en Windows.

- 2026-09-10: `bridge::replay_page_ops` ya no devuelve solo el documento, sino
  `ReplayOutcome { document, page_objects }`. El mapa se resuelve donde se
  establece la invariante en la que descansa —cada paso del replay reconstruye
  `working` con exactamente el orden de `current`—, en vez de re-derivarlo más
  tarde desde un documento que el llamante no puede distinguir del que produjo
  el replay. `strategy::save_full_rewrite` lo consume tal cual.
- `bridge::page_annotation_objects` toma ahora ese mapa y lee el documento que
  se le pasa, en vez de recorrer posiciones. Motivo: recorrer posiciones
  significaba leer las anotaciones existentes desde `base`, y una página
  importada no está en `base` en absoluto. Su `PageId` no encontraba entrada,
  `attach_annotations` partía de una lista vacía y el `dict.set("Annots", …)`
  final sustituía el `/Annots` que el injerto había traído consigo. La ruta
  incremental pasa el mapa derivado de `base` y se comporta igual que antes.
- Regresión fijada en `core/pdf-save/tests/imported_page_annotations.rs`: una
  página importada conserva sus anotaciones; una anotación nueva sobre esa
  página se añade en vez de sustituirlas; y una página base mantiene las suyas
  cuando una importación la desplaza. La segunda fallaba antes del cambio
  (1 anotación en vez de 2).
- Verificación (2026-09-10): `cargo test --workspace --locked` (0 fallos,
  ejecutado en Windows, por lo que `linux-gtk` no entra en la compilación),
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo fmt --all -- --check` y `python scripts/check_maintainability.py`
  (104 avisos, línea base sin cambios; la nota de secciones anteriores que
  cita 99 quedó desactualizada). La suite GTK4 no se ejecutó en esta entrega.

- 2026-09-10: nuevo módulo `core/pdf-save/src/origin.rs`. `page_backing`
  responde de qué documento y de qué objeto salen los bytes de una página
  usando su `PageOrigin`: la base para una página del archivo abierto, el PDF
  de origen para una importada, y `PageBacking::Empty` para una en blanco, que
  no tiene objeto en ninguna parte hasta que un guardado la materializa.
  Encima viven `read_page_content_of` y `page_font_families_of`.
- `pdf-edit` gana las puertas equivalentes por objeto —
  `read_page_object_content` y `page_object_font_families` (esta última en el
  nuevo `parse/fonts.rs`)—. La resolución posicional sigue existiendo para
  quien la quiera, pero ya no es el único camino.
- `content::validate_content_command` toma ahora el modelo y el registro de
  fuentes, y prueba el comando contra el documento que la página nombra. Antes
  clonaba `base` y buscaba la página por posición, algo que no podía alcanzar
  una página importada: su `PageId` se asigna por encima del de cualquier
  página base.
- Un ítem leído del PDF de origen sigue resolviendo contra la página que el
  injerto produce: `graft_pages` copia el stream de contenido tal cual y
  conserva los nombres de recurso, así que las posiciones del parse coinciden.
  Eso no se asume, se fija en
  `core/pdf-save/tests/imported_page_content_edit.rs`
  (`editing_an_imported_pages_text_reaches_the_saved_file`).
- En Linux, `content_edit::base_page` pasa a llamarse `content_page` y acepta
  páginas importadas; la nueva `content_edit::page_probe` traduce un `PageId`
  al par documento/objeto contra el que se parsea y se prueba. Los diez
  validadores de `content_edit::command` y `model::ensure_page_content` toman
  ese par en vez de `(base, PageId)`, con lo que desaparecen las diez llamadas
  a `pdf_edit::page_object_id` repartidas por el shell.
- `ImportedSources` pasa a tener dos tiempos de vida (`'s` para el slice, `'d`
  para los documentos). Con uno solo, una referencia resuelta quedaba atada a
  la vida de un registro temporal, y el shell no podía construir el suyo como
  local sin sostener andamiaje alrededor. Por el mismo motivo `page_probe`
  toma el modelo con su propio tiempo de vida: nada del resultado apunta a él,
  y atarlo dejaría `session.document_model` prestado justo cuando el llamante
  lo necesita mutable.
- `pdf-ffi` enruta `read_page_content`, `page_font_families` y la validación
  por el mismo origen, con `ImportedSources::none()`: ese handle no tiene
  registro de importaciones —importar es del shell Linux por ahora—, así que
  una página importada se rechaza con un mensaje claro en lugar de leer en
  silencio la página base que ocupe ese índice.
- Los ocho tests de `content_edit::command` que fijaban "un `PageId` sin
  página se rechaza" se retiraron: esa garantía no desapareció, se mudó a
  `page_probe`. La cubren ahora dos tests GTK en
  `app/organize/tests/resolution.rs` (una página importada resuelve contra su
  fuente; una sin fuente registrada no obtiene probe) y, en el núcleo,
  `validation_still_refuses_a_command_the_save_could_not_replay`.
- Verificación (2026-09-10, en WSL2/Ubuntu con `PDFIUM_DYNAMIC_LIB_PATH`
  apuntando al `libpdfium.so` vendorizado): `cargo test --workspace --locked`
  (0 fallos), `cargo test -p linux-gtk --locked` (356 aprobadas, suite GTK4
  incluida), `cargo clippy --workspace --all-targets --locked -- -D warnings`.
  En Windows: `cargo fmt --all -- --check` y
  `python scripts/check_maintainability.py` (104 avisos, línea base sin
  cambios — `parse/mod.rs` cruzó el umbral de 350 líneas al ganar las puertas
  por objeto y se dividió en `parse/fonts.rs` para devolverlo). El empaquetado
  y smoke test de Linux sigue sin verificarse.

### Progreso de la validación y el ciclo completo

- 2026-09-10: `document::reopened_matches_model` compara el handle que PDFium
  acaba de abrir contra el modelo con el que se escribieron esos bytes, antes
  de que nada lo instale. Abrir ya validaba bastante — un archivo ilegible
  falla en `open_document` y su barrido de `page_sizes` toca todas las
  páginas — pero "abrió" no es "coincide": la previsualización instala
  `backend_pages` desde el modelo *preservado*, así que cada índice de lienzo
  se resuelve por el orden de páginas del modelo dando por hecho que el handle
  reabierto tiene exactamente esas páginas. Con un recuento distinto, ese
  supuesto falla en silencio y las páginas se dibujan, se prueban y aceptan
  anotaciones bajo la identidad de otra.
- La misma puerta cubre las dos rutas: `refresh_snapshot_and_reopen` cierra el
  handle y devuelve el error (la previsualización anterior se queda con una
  explicación, igual que ante cualquier guardado fallido), y
  `save_snapshot_and_reopen` la aplica **antes** de `atomic_write`, con
  `page_count`, para no sustituir un archivo por bytes cuyo recuento de páginas
  no es el del modelo.
- El ciclo completo se fija en `core/pdf-save/tests/assembly_reopen_roundtrip.rs`:
  una sesión importa, mueve y borra, guarda, y las siguientes pruebas
  reabren esos bytes como documento nuevo — sin modelo heredado y sin registro
  de fuentes. La invariante que importa tras reabrir es que el archivo se
  sostiene solo: la página importada ya es parte del PDF (`PageOrigin::Base`),
  el segundo guardado funciona con `ImportedSources::none()`, y el documento
  reabierto vuelve a aceptar operaciones de página.

#### Bug encontrado por esa prueba: el reorden se perdía tras un borrado

- `pdf_manip::delete_pages` llama a `renumber_objects`, así que después de
  borrar una página ningún `ObjectId` de `base` nombra nada en el documento de
  trabajo. `bridge::replay_page_ops` resolvía los supervivientes justamente
  así: `PageId` → `ObjectId` de `base` → número de página posterior al borrado.
  Tras un borrado ese segundo salto no encontraba nada, `survivor_target_order`
  salía **vacío**, y la comprobación `windows(2)` sobre un slice vacío es
  falsa — el reorden nunca llegaba a ejecutarse. El bug se escondía a sí mismo:
  `reorder_pages` habría rechazado esa permutación vacía, pero nunca se le
  llamaba.
- Mismo origen, segundo síntoma: el paso de rotaciones nombraba el número de
  página por la misma vía y salía por su `continue` de "esto no debería pasar",
  así que un guardado que borrara y rotara a la vez no rotaba nada.
- Arreglo: los números de página de los supervivientes se **cuentan**, no se
  buscan por identidad de objeto. Lo que sobrevive a un borrado es el orden
  —las páginas conservadas quedan en orden de `base`—, así que el número
  posterior al borrado de un superviviente es su posición 1-based entre las
  páginas de `base` que se conservaron. Y tras el paso 2 el documento de
  trabajo contiene exactamente los supervivientes en el orden de `current`,
  con lo que la rotación usa la posición en ese recorrido. `id_to_object`
  desaparece: no hacía falta ninguna de las dos veces.
- Tres regresiones en `bridge`: borrar + reordenar, borrar + rotar, y las tres
  operaciones juntas (borrado, reorden e injerto en un mismo guardado). Las
  tres fallaban antes del arreglo.
- Verificación (2026-09-10, en WSL2/Ubuntu con `PDFIUM_DYNAMIC_LIB_PATH`
  apuntando al `libpdfium.so` vendorizado): `cargo test --workspace --locked`
  (1300 aprobadas, 0 fallidas, suite GTK4 incluida), `cargo test -p linux-gtk
  --locked` (360 aprobadas), `cargo clippy --workspace --all-targets --locked
  -- -D warnings`. En Windows: `cargo fmt --all -- --check`, `git diff --check`
  y `python scripts/check_maintainability.py` (104 avisos, línea base sin
  cambios). El smoke de empaquetado (`package_smoke`, 2 aprobadas) sí corrió
  esta vez: esta copia tiene el `libpdfium.so` vendorizado, que es lo que
  faltaba en las entregas anteriores. La suite GTK4 corrió bajo WSLg con
  display real, no bajo `xvfb-run`; ese gate concreto es el de CI.

### Progreso del refresco de PDFium

- 2026-09-08: `DocumentSession` guarda `backend_pages`, el orden de páginas
  del handle de PDFium abierto, con `backend_index` y `backend_page_id` como
  únicas formas de convertir entre `PageId` e índice de canvas. Se instala en
  `document::show_document` y se reinstala en `document::restore_edit_state`
  desde el modelo con el que se escribieron los bytes reabiertos.
- `document::refresh_after_content_edit` pasó a llamarse `refresh_preview`:
  el ciclo guardar-en-memoria + reabrir ya no es exclusivo de la edición de
  contenido. `organize::command` lo dispara después de mover o borrar una
  página, y `annotations::command::history` después de deshacer o rehacer una
  operación de página. El mensaje de estado dejó de ser `&'static str` para
  admitir el texto dinámico de esas operaciones.
- La rejilla de Organizar pide cada miniatura por la posición de la página en
  el handle abierto, no por `page.id.0`. Una página que el handle todavía no
  tiene — una inserción cuyo refresco no llegó — conserva su marcador.
- Dibujo, hit-testing y colocación de anotaciones y campos resuelven el índice
  de canvas a `PageId` una sola vez por página en lugar de comparar `.0` por
  elemento. Una página ausente del handle no dibuja nada y rechaza la
  colocación en vez de aplicarla a la página que heredó ese número.
- La edición de contenido convierte una vez, en `content_edit::base_page`, y
  desde ahí viaja como `PageId`: `ensure_page_content` y los diez validadores
  de `content_edit::command` dejaron de aceptar un índice de canvas. Una
  página sin página base — en blanco hoy, importada mañana — devuelve `None` y
  la edición se rechaza. Los validadores que reciben un item usan `item.page`,
  que ya es la identidad correcta.
- Gotcha registrada: el orden importaba. Refrescar PDFium sin migrar antes
  esos consumidores habría roto anotaciones, formularios y edición de
  contenido después de cualquier reordenamiento sin guardar — el lienzo habría
  pasado al orden nuevo mientras cada consumidor seguía leyendo `PageId.0`
  como posición del orden viejo.
- Verificación (2026-09-08, este cambio, ejecutada en WSL2/Ubuntu):
  `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets
  --locked -- -D warnings`; `cargo test --workspace --locked` (1130 aprobadas,
  0 fallidas, suite GTK4 incluida — 348 en `linux-gtk`);
  `python3 scripts/check_maintainability.py` (99 avisos, igual que la línea
  base previa al cambio).

## 7. Actualización de la sesión Linux

- [x] Generalizar el ciclo de guardar en memoria y reabrir usado por edición de
  contenido.
- [x] Mantener el historial completo durante la actualización de la
  previsualización.
- [x] Mantener el estado de cambios sin guardar.
- [x] Mantener selecciones y contadores de identificadores que sigan siendo
  válidos.
- [x] Invalidar selecciones y cachés que apunten a páginas eliminadas.
- [x] Instalar conjuntamente el nuevo handle de PDFium y su mapa de páginas.
- [x] Evitar que un resultado asíncrono antiguo reemplace una sesión más nueva.
- [x] Conservar la sesión anterior intacta si falla la materialización o la
  reapertura.

### Progreso de la actualización de sesión

- 2026-09-09: `refresh_preview` conserva el mismo `Document` y con él su
  `EditLog`, mantiene `unsaved_to_disk`, e instala el handle reabierto junto al
  orden de `PageId` usado para materializar sus bytes. `SessionToken` impide
  instalar resultados con una generación o revisión obsoleta y la sesión solo
  se sustituye después de completar materialización y reapertura, por lo que un
  fallo deja el handle y el estado anteriores intactos.
- 2026-09-09: `EditState` conserva conjuntamente los contadores y selecciones
  de anotaciones y formularios. Al restaurar, las selecciones se filtran contra
  el mismo modelo preservado y contra la presencia de su página antes de
  reactivar sus controles, evitando dejar inspectores dirigidos a objetos o
  páginas que ya no existen. Selección de texto,
  imágenes, búsqueda y cachés permanecen fuera: todavía usan posiciones del
  backend o datos del handle reemplazado y requieren una política explícita de
  remapeo o invalidación antes de cerrar los dos ítems pendientes.
- 2026-09-10: la invalidación pasó a ser una regla explícita en lugar de un
  efecto secundario. `EditState` (`apps/linux-gtk/src/app/document.rs`)
  documenta el criterio: **un campo viaja a través del refresco solo si su
  clave sobrevive a la reapertura**. Los `AnnotationId`/`FormFieldId` la
  sobreviven (y aun así pasan por `surviving_edit_selections`, porque
  sobrevivir a la reapertura no es lo mismo que sobrevivir al borrado de una
  página); la selección de texto, las coincidencias de búsqueda, la imagen o
  el editor de contenido abiertos, los arrastres en vuelo y las cachés de
  render están todos indexados por posiciones del handle reemplazado y mueren
  con la sesión que `show_document` descarta.
- 2026-09-10: `stamp_surfaces` sí viaja, y antes no lo hacía. Es una caché de
  decodificación indexada por `AnnotationId`, no una posición, y la anotación
  cuyos bytes guarda sigue en el modelo preservado. Al perderse, cada sello
  colocado por el usuario se dibujaba como un contorno vacío (el fallback de
  `selection::draw_annotation` para un sello sin superficie) desde el primer
  movimiento de página en adelante.
- 2026-09-10: el hueco real detrás de este ítem no estaba en el shell sino en
  el núcleo. `Command::RemovePage` sacaba **solo la página**: las anotaciones
  y los campos de formulario anclados a ella quedaban en el modelo nombrando
  un `PageId` inexistente, y tanto `pdf_save::attach_annotations` como
  `write_form_fields` rechazan ese guardado con `InvalidSaveRequest`. Es
  decir, subrayar una página y borrarla dejaba el documento **imposible de
  guardar** — fallaban por igual el refresco de previsualización y cada
  Ctrl+S — hasta deshacer el borrado.
- El comando ahora lleva consigo lo que había en la página, y su inverso lo
  devuelve. `AnnotationSet::take_page`/`restore` y sus gemelos en
  `FormFieldSet` registran la **posición** de cada elemento, no solo su valor:
  el orden de esos conjuntos es orden de pintado y está fijado por el check de
  paridad byte a byte, así que un deshacer que los agregara al final
  reapilaría la página en silencio. `Command::remove_page(document, index)` es
  la única forma correcta de construirlo; capturar la página sin lo que hay
  sobre ella es exactamente el bug que estos campos evitan.
- Aviso de mantenibilidad: 101 avisos frente a los 99 de la línea base. Los
  dos nuevos son `core/pdf-document/src/form.rs` (395 líneas, cruza el umbral
  de 350 al sumar el par `take_page`/`restore` y sus tres pruebas) y
  `core/pdf-document/src/edit_log.rs` (42 puntos de decisión, cruza el de 40
  porque las ramas `InsertPage`/`RemovePage` de `apply` ganaron una llamada
  cada una). Ambos archivos son una sola familia de tipos y un único `match`
  exhaustivo respectivamente; dividirlos es un refactor que no debe viajar
  junto a un arreglo de comportamiento. `apps/linux-gtk/src/app/ui_tests.rs`
  sí se dividió, porque ahí el crecimiento sí era una segunda
  responsabilidad: los fixtures de modelo se fueron a
  `apps/linux-gtk/src/app/test_fixtures.rs`.
- Verificación (2026-09-10, en WSL2/Ubuntu): `cargo test --workspace --locked`
  (1273 aprobadas, 0 fallidas, suite GTK4 incluida), `cargo clippy --workspace
  --all-targets --locked -- -D warnings`, `cargo fmt --all -- --check` y
  `python3 scripts/check_maintainability.py`. La suite GTK4 corrió bajo el
  display Wayland real de WSLg, no bajo el gate `xvfb-run` que documenta
  CONTRIBUTING.md: `xvfb-run` no está instalado en esta copia y instalarlo
  requiere `sudo`. Ese gate concreto queda **sin verificar localmente**. La
  prueba
  `deleting_an_annotated_page_leaves_the_document_saveable`
  (`core/pdf-save/tests/save_roundtrip.rs`) se comprobó en rojo revirtiendo
  temporalmente la captura en `apply` antes de darla por buena. El empaquetado
  y smoke test de Linux siguen sin verificarse: esta copia no trae
  `libpdfium.so` empaquetado para ese gate.
- Verificación (2026-09-09, estado lógico durante el refresco): en WSL2/Ubuntu,
  `cargo test -p linux-gtk --locked -- --skip
  package_smoke::tests::renders_the_embedded_sample_to_a_nonempty_receipt` (351
  aprobadas, 1 filtrada) y `cargo clippy -p linux-gtk --all-targets --locked --
  -D warnings`; en el workspace, `cargo fmt --all -- --check`, `git diff
  --check` y `python scripts/check_maintainability.py` (99 avisos, línea base
  sin cambios). El package smoke completo sigue sin verificarse porque esta
  copia no contiene `libpdfium.so` para Linux.

## 8. Selección e importación en Linux

- [x] Añadir `Add PDFs` a la cabecera de Organizar.
- [x] Permitir selección múltiple mediante `FileDialog::open_multiple`.
- [x] Respetar el orden devuelto por la selección de archivos.
- [x] Ejecutar lectura, validación e importación fuera del hilo principal.
- [x] Mostrar progreso para operaciones perceptibles.
- [x] Permitir cancelar la selección, el worker, la solicitud de contraseña o
  la confirmación sin modificar documento, historial ni estado sucio.
- [x] Mostrar errores por archivo con una explicación accionable.
- [x] Mostrar antes de confirmar lo que la importación deja atrás
  (`pdf_manip::graft_report`): enlaces que dejan de resolver, marcadores y
  estructura etiquetada. Sin esto, la sección 4 ítem 9 queda sin cerrar.
- [x] Añadir inicialmente cada PDF como un bloque al final del documento.
- [x] Actualizar controles de guardado, deshacer y rehacer tras importar.

### Progreso de selección e importación

- 2026-09-10: Linux añade `Add PDFs` y usa `FileDialog::open_multiple`. El
  orden de `gio::ListModel` se conserva al abrir cada archivo y al concatenar
  sus páginas en un único `Command::ImportPages`, por lo que toda la selección
  es una sola operación de deshacer y rehacer.
- La sesión posee un registro de `ImportedDocumentId` a `LopdfDocument`. Ese
  registro viaja por el refresco de previsualización y se entrega a todas las
  rutas de guardado; al reabrir un guardado real se vacía porque las páginas ya
  forman parte del nuevo PDF base.
- Apertura, permiso de ensamblado, `graft_report` y construcción de páginas se
  ejecutan mediante `gio::spawn_blocking`. Un `SessionToken` descarta el
  resultado si el documento o su revisión cambian mientras trabaja.
- La cabecera muestra progreso por archivo y una acción Cancelar mientras el
  worker está activo. La cancelación es cooperativa: se comprueba entre lectura,
  descifrado, análisis e importación de cada fuente; una llamada síncrona de
  lopdf que ya está ejecutándose termina antes de observarla.
- Cada fuente cifrada solicita y conserva su propia contraseña dentro de la
  operación transitoria. Un fallo reabre el diálogo para esa misma fuente y no
  reutiliza ni modifica la contraseña del documento principal. Cancelar el
  diálogo cancela el lote completo sin mutar la sesión.
- La importación se rechaza completa ante el primer archivo inválido, sin
  páginas, sin permiso de copiar/extraer o con estructura no soportada. Una
  fuente cifrada pausa el lote para solicitar su contraseña; las advertencias
  de todos los archivos se presentan juntas y Cancelar no registra fuentes ni
  comandos.
- Verificación local: `cargo test -p pdf-document --locked` (145 aprobadas),
  `cargo test -p pdf-save --locked` (todas aprobadas; 4 pruebas explícitamente
  ignoradas), `cargo clippy -p pdf-document -p pdf-save --all-targets --locked
  -- -D warnings` y `cargo fmt --all -- --check`. En WSL2/Ubuntu,
  `cargo test -p linux-gtk --locked -- --skip package_smoke::tests::
  renders_the_embedded_sample_to_a_nonempty_receipt` (355 aprobadas, 1
  filtrada) y los 8 tests `app::organize::tests::gtk_ui_` pasaron. El smoke
  empaquetado sigue sin verificarse porque esta copia no contiene
  `libpdfium.so` para Linux.

## 9. Vista por documentos

- [x] Añadir el selector visual `Documents | Pages`.
- [x] Abrir Organizar con `Documents` seleccionado.
- [x] Mostrar cada tramo contiguo como una tarjeta de documento.
- [x] Incluir portada apilada, nombre, número de páginas y rango actual.
- [x] Mantener una jerarquía visual limpia y coherente con la paleta existente.
- [x] Permitir arrastrar una tarjeta para mover todo el tramo.
- [x] Resolver inserciones antes, después y al final de la lista.
- [x] Mostrar claramente el destino durante el arrastre.
- [x] Permitir eliminar un bloque mediante una única acción reversible.
- [x] Añadir nombres accesibles y ayudas de teclado para mover y eliminar.
- [x] Evitar depender exclusivamente de arrastrar y soltar.

### Progreso de la vista por documentos

- 2026-09-11: la pantalla Organizar tiene dos vistas dentro de un `GtkStack`
  (`apps/linux-gtk/src/app/organize/views.rs`): `documents` y `pages`. El
  selector son dos `ToggleButton` agrupados, no un `StackSwitcher` — el
  switcher toma los títulos del propio stack y no deja decidir orden ni
  estilo, y las dos vistas no pesan igual: `organize::show` siempre entra por
  `Documents`. Cambiar de vista solo repuebla widgets: no registra comandos ni
  ensucia la sesión (test `gtk_ui_switching_views_records_no_command_...`).
- Cada tarjeta es un `Block` de `pdf_document::derive_blocks`, derivado en
  cada repoblado. No hay identidad de tarjeta que preservar: un movimiento
  puede fusionar dos tramos o partir uno, así que la lista se reconstruye
  entera. Es un render pdfium por *documento*, no por página.
- La tarjeta lleva portada apilada (dos hojas decorativas detrás de la
  miniatura real de su primera página, ocultas si el bloque tiene menos
  páginas), nombre, recuento y rango actual — `base.pdf`, `3 pages · 3–5` — y
  `— Part N` cuando el núcleo marcó el tramo como dividido. El nombre viene de
  `DocumentSession::base_name` (nuevo, capturado al abrir y preservado a
  través del refresco de previsualización) o de `ImportedSource::name` (nuevo,
  capturado al importar).
- Los destinos de arrastre son los *huecos* entre tarjetas, uno más que
  tarjetas hay: antes del primer bloque, entre cada par y después del último.
  Un destino del tamaño de una tarjeta tendría que adivinar "antes o después"
  por la posición del puntero dentro de ella y no tendría dónde mostrar la
  respuesta; un hueco *es* la posición, así que resaltarlo dice exactamente
  dónde cae el bloque. El payload del arrastre es el `PageId` ancla del
  bloque, no su posición: el destino se resuelve contra los bloques tal como
  están al soltar.
- Mover un bloque es un único `Command::MovePages`; borrarlo es un único
  `Command::RemovePages` — nuevo en `pdf-document`, el gemelo por tramo de
  `RemovePage`, con su constructor `Command::remove_pages(document, index,
  count)` que captura anotaciones y campos de formulario de *todas* las
  páginas del tramo, y su inverso `Command::InsertPages`. Sin él, borrar un
  bloque habría dejado anotaciones huérfanas apuntando a páginas que
  `pdf-save` se niega a escribir.
- Alternativa al arrastre (no solo accesibilidad: es la ruta de teclado):
  cada tarjeta lleva botones `Move up`, `Move down` y `Delete`, con nombre
  accesible y tooltip propios (`Move up: report.pdf`), insensibles cuando el
  movimiento saldría de la lista. La tarjeta misma es enfocable y anuncia
  `"report.pdf, 3 pages · 3–5, document 2 of 4"`.
- El repoblado tras un comando ocurre en la propia vista, no solo en
  `refresh_after_reopen`: ese refresco se cancela solo cuando la sesión no
  tiene `save_backing` con el que reproducir, y la lista se quedaría mostrando
  los bloques viejos. Pagar dos renders por bloque es lo que la caché de
  miniaturas de §11 viene a resolver.
- Verificación (WSL2/Ubuntu): `cargo test -p linux-gtk --locked` (388
  aprobadas, incluidas las 2 de `package_smoke`), `cargo test --workspace
  --locked` (1337 aprobadas, 7 ignoradas) y `cargo clippy -p linux-gtk
  --all-targets --locked -- -D warnings`. En Windows: `cargo test --workspace
  --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo fmt --all -- --check`, `python scripts/check_maintainability.py`.
- Dos gates no se ejecutaron, y la razón que arrastraban las notas anteriores
  era incorrecta: `libpdfium.so` **sí** está vendorizado en esta copia (las
  pruebas `package_smoke` del crate pasan). Lo que falta es otra cosa —
  `scripts/package-linux.sh` exige `PDFIUM_ARCHIVE`, el tarball de release
  verificado, que no está acá, así que el empaquetado `.deb`/`.AppImage` y su
  `verify-linux-package.sh` siguen dependiendo de CI. Y `xvfb-run` no está
  instalado en esta WSL: la suite GTK corrió bajo WSLg con display real, no
  por la ruta headless que cubre el workflow `linux-gtk-ui`.

## 10. Vista por páginas

- [x] Reutilizar la cuadrícula individual existente sin duplicar decisiones de
  negocio.
- [x] Permitir mezclar páginas de diferentes PDFs.
- [x] Mantener reordenación y eliminación como operaciones reversibles.
- [x] Transferir identidades estables durante el arrastre, no índices
  capturados.
- [x] Resolver inserciones en huecos y después de la última tarjeta.
- [x] Actualizar números de página después de cada operación.
- [x] Mostrar la procedencia de una página sin sobrecargar visualmente la
  tarjeta.
- [x] Verificar que volver a `Documents` conserva exactamente el orden actual.

### Progreso de la vista por páginas

- 2026-09-11: la cuadrícula existente se conservó entera — sigue siendo un
  `FlowBox` ordenado por `Cards`, sin reconstrucción en cada movimiento y con
  un render pdfium por página. Lo que cambió es qué viaja en el arrastre y
  cómo se resuelve el destino. `apps/linux-gtk/src/app/organize/grid.rs` se
  partió por responsabilidad al crecer: `grid/card.rs` (qué es una tarjeta),
  `grid/drop.rs` (dónde cae la que se arrastra), `grid/thumbnail.rs` (el
  render), igual que `documents/` en §9.
- El arrastre lleva el `PageId` de la página, no la posición que tenía su
  tarjeta al empezar el gesto, y el drop resuelve ese identificador contra
  `Document.pages` tal como está al soltar. Un índice capturado solo es
  correcto mientras nada más se mueva, y los botones de deshacer y rehacer
  están en la cabecera de esta misma pantalla. `Card` ganó el campo `id`, que
  es lo que prepara su `DragSource`.
- El destino es un *slot* de inserción: `k` significa "antes de la página que
  hoy está en `k`" y `cards.len()` significa "después de la última", así que
  soltar más allá de la última tarjeta ya tiene respuesta — antes
  `child_at_pos` devolvía `None` ahí y el drop se perdía en silencio. No hay
  widgets de hueco como en §9: las tarjetas de esta vista son los hijos de un
  `FlowBox` homogéneo y un hueco tendría que ser hijo también, ocupando una
  celda y reacomodando las filas. El slot sale de qué mitad de tarjeta tiene
  el puntero encima (`slot_at`), lo que hace que el espacio entre columnas y
  el espacio entre filas resuelvan a la misma posición sin caso especial, y
  se dibuja como un acento en el borde cercano de esa tarjeta
  (`.organize-card-drop-before` / `-after`, sombra interior para que
  encenderla no cambie el tamaño de la tarjeta a mitad del arrastre).
- Mover y borrar siguen siendo `Command::MovePage` y `Command::remove_page`
  —un paso de deshacer cada uno— y la renumeración posterior sigue siendo
  texto, nunca un render.
- La procedencia es una línea bajo la miniatura, elidida al medio y con el
  nombre completo en el tooltip. Aparece solo cuando la lista de páginas
  tiene más de una fuente: nombrar el único PDF en las cincuenta tarjetas de
  un documento sin combinar es ruido, que es exactamente lo que el checklist
  pide evitar. Borrar la última página importada vuelve a dejar una sola
  fuente, así que `relabel_sources` corre también después de un borrado. Qué
  nombre corresponde a qué fuente es una sola decisión: vive en
  `documents::source_name` y las dos vistas la leen de ahí.
- Verificación (WSL2/Ubuntu): `cargo test -p linux-gtk --locked` (403
  aprobadas, incluidas las 2 de `package_smoke`), `cargo test --workspace
  --locked` (1351 aprobadas, 7 ignoradas), `cargo clippy --workspace
  --all-targets --locked -- -D warnings`, `cargo build --workspace --locked`,
  `cargo fmt --all -- --check` y `python3 scripts/check_maintainability.py`
  (ninguna advertencia nueva sobre los archivos tocados; `organize/import.rs`
  sigue por encima del umbral desde §8 y no se tocó en este cambio).
- Siguen sin ejecutarse los mismos dos gates que en §9, por las mismas
  razones: `scripts/package-linux.sh` exige `PDFIUM_ARCHIVE`, el tarball de
  release verificado, que no está en esta copia, y `xvfb-run` no está
  instalado en esta WSL, así que la suite GTK corrió bajo WSLg con display
  real y no por la ruta headless del workflow `linux-gtk-ui`.

## 11. Animación y rendimiento

- [x] Construir vistas independientes dentro de un `GtkStack`.
- [x] Evitar reparentar widgets retirados de un `FlowBox`.
- [x] Animar el paso de documentos a páginas con opacidad, escala y
  desplazamiento.
- [x] Escalonar la aparición de páginas para comunicar que salen de sus
  bloques.
- [x] Limitar la duración total de la secuencia en documentos grandes.
- [x] Animar solo elementos visibles cuando hacerlo completo sea costoso.
- [x] Respetar la configuración global de animaciones de GTK.
- [x] Cambiar inmediatamente de vista cuando las animaciones estén
  desactivadas.
- [x] Cachear miniaturas por documento, página, tamaño y factor de escala.
- [x] Invalidar resultados de renderizado obsoletos mediante una generación.
- [x] Evitar renderizar nuevamente todas las miniaturas al cambiar de vista.
- [ ] Medir importación, primer render y cambio de modo con documentos grandes.
  Parcial: las tres operaciones están medidas en renders de pdfium, que es lo
  que cuesta caro en ellas, pero no cronometradas sobre un PDF real grande.

### Progreso de la animación y el rendimiento

- 2026-09-11: las dos vistas ya vivían en un `Stack` propio
  (`OrganizePanel::views`, §9) y el reorden de la cuadrícula ya evitaba
  reparentar hijos de un `FlowBox` desde §10 — mueve una clave de orden, no
  un widget. Los dos primeros ítems se cierran sobre ese código, no sobre
  código nuevo; lo que se añadió al `Stack` es su transición.
- La caché de miniaturas es `apps/linux-gtk/src/app/organize/cache.rs`
  (`Thumbnails` + `ThumbnailKey`), un campo más de `OrganizePanel`. La clave
  es `PageId` + tamaño lógico + factor de escala, **no** el índice de página
  del handle pdfium: ese número lo renumeran `Command::MovePage`, una
  importación y cada reapertura de `document::refresh_preview`, así que una
  caché apoyada en él le daría a una página movida la foto de la que heredó
  su hueco. El "por documento" del checklist lo da el ciclo de vida: los
  `PageId` son únicos dentro de un modelo y vuelven a empezar en 0 en el
  siguiente, por lo que `organize::document_changed` vacía la caché antes de
  que el documento nuevo pida nada.
- El tamaño está en la clave porque las dos vistas piden las mismas páginas
  a medidas distintas (`grid::card::CARD_WIDTH_PX` contra
  `documents::card::COVER_WIDTH_PX`), y una entrada compartida dibujaría la
  portada de un bloque con píxeles renderizados para una tarjeta vez y media
  más ancha.
- Hay presupuesto en bytes (96 MiB, LRU) y no un mapa sin fondo: una página
  A4 a la medida de la vista Pages son unos 380x538 px —cerca de 0,8 MB— y
  cuatro veces eso en HiDPI, así que guardar las 500 páginas de un ensamblado
  serían cientos de megabytes de píxeles de tarjetas que nadie está mirando.
  Dentro del presupuesto un cambio de vista cuesta cero renders; por encima
  degrada a renderizar lo que se cayó, que sigue siendo estrictamente menos
  que el re-render completo que sustituye.
- La invalidación es un contador de generación y no un borrado de claves. Un
  render se pide en el hilo principal y aterriza varios frames después; entre
  medias una edición de contenido puede repintar justo esa página, y eso deja
  mal a la vez el resultado en vuelo y todas las entradas guardadas. Subir el
  contador responde las dos cosas de una: la caché se vacía y el render que
  empezó antes del salto ve que ya no es actual y descarta su resultado en
  lugar de volver a llenar la caché con los píxeles que la edición acaba de
  rechazar. `organize::invalidate_thumbnails` es quien lo sube.
- La animación vive en `apps/linux-gtk/src/app/organize/motion.rs` y se
  aplica desde `views::show`, nunca desde `populate`: a los dos `populate`
  los llaman también un deshacer, un rehacer, un movimiento de bloque y un
  refresco de vista previa, y animar eso haría que cada Ctrl+Z parpadeara la
  cuadrícula entera. Nada retira las clases después y nada tiene por qué:
  toda llegada reconstruye las tarjetas que anima, así que la clase siempre
  cae sobre un widget que no la llevaba.
- La opacidad de conjunto la pone el `Stack` (`Crossfade`, 160 ms) y la
  escala y el desplazamiento las ponen los keyframes `organize-enter` de cada
  tarjeta (`translateY(10px) scale(0.96)` hasta nada, 180 ms). El escalonado
  son doce clases `.organize-enter-N` con retardos de 20 ms, porque GTK4 CSS
  no tiene estilos en línea y un retardo por tarjeta tiene que ser una clase
  que la hoja ya nombre. Ese tope es justo lo que cierra los dos ítems de
  coste: la secuencia no puede durar más de 220 ms + 180 ms por muchas
  páginas que tenga el documento, y las tarjetas más allá de la última
  ranura no animan. Son doce con cinco tarjetas por fila, algo más de dos
  filas: una aproximación al conjunto visible por presupuesto fijo, no una
  consulta real de la posición del scroll.
- `gtk-enable-animations` se consulta en cada llegada y no una vez al
  construir la pantalla: es un ajuste vivo, y quien lo apaga espera que el
  siguiente cambio de vista sea instantáneo, no el siguiente arranque. Con él
  apagado no se añade ninguna clase —las tarjetas están sin más— y GTK se
  salta además la transición del `Stack` por su cuenta.
- Medición (ítem 12, parcial). La parte cara de mostrar cualquiera de las dos
  vistas es un render de pdfium por tarjeta, y el banco de pruebas ya cuenta
  exactamente eso en la frontera donde se piden
  (`organize::tests::THUMBNAILS`), así que las tres operaciones se miden en
  renders y no en un cronómetro que dependa de la máquina:
  `gtk_ui_switching_between_the_two_views_renders_nothing_again` (tres idas y
  vueltas sobre 12 páginas: 0 renders nuevos; antes eran 12 por cada
  entrada), `gtk_ui_importing_pages_does_not_re_render_the_grid_it_lands_in`
  (0 sobre las páginas que ya estaban) y el primer render, que sigue siendo
  uno por tarjeta y está fijado en el mismo test. Lo que **no** se hizo es
  cronometrar un PDF real de varios cientos de páginas: no hay fixture de ese
  tamaño en el repositorio y esta caja no puede pintar la ventana GTK
  (`use_gfxredir = 0` en WSLg), así que cualquier número de reloj salido de
  aquí sería ruido. El ítem queda abierto.
- La hoja de estilos se comprobó cargada sin advertencias de GTK —`@keyframes`,
  `transform` y `animation-*` los acepta GTK4 sin quejarse—, pero el resultado
  **no** se verificó visualmente: esta caja pinta toda el área de contenido en
  negro bajo WSLg, un fallo del entorno ya documentado y ajeno al código. Lo
  que las pruebas fijan es qué tarjetas llevan qué clases y cuántas, no cómo
  se ven a mitad de vuelo; un `#[gtk::test]` no tiene reloj de frames que
  muestrear.
- Verificación (WSL2/Ubuntu): `cargo test -p linux-gtk --locked` (420
  aprobadas), `cargo test --workspace --locked` (1368 aprobadas, 7
  ignoradas), `cargo clippy --workspace --all-targets --locked -- -D
  warnings`, `cargo build --workspace --locked`, `cargo fmt --all -- --check`
  y `python3 scripts/check_maintainability.py` (sin advertencias nuevas:
  `cache.rs` y `motion.rs` quedan por debajo del umbral, y `import.rs` y
  `state.rs` ya avisaban desde antes de este cambio).
- Los dos gates que siguen sin ejecutarse son los mismos de §9 y §10, por las
  mismas razones: `scripts/package-linux.sh` exige `PDFIUM_ARCHIVE`, que no
  está en esta copia, y `xvfb-run` no está instalado en esta WSL, así que la
  suite GTK corrió bajo WSLg con display real y no por la ruta headless del
  workflow `linux-gtk-ui`.

## 12. Pruebas del núcleo

- [x] Probar que importar un PDF es un único paso de historial.
- [x] Probar que deshacer y rehacer restaura orden y procedencia.
- [x] Probar que mover un bloque es un único paso de historial.
- [x] Probar importación de texto, imágenes y recursos anidados.
- [ ] Probar páginas con atributos heredados y árboles de páginas anidados.
- [x] Probar colisiones de identificadores entre documentos.
- [x] Probar enlaces entre páginas importadas.
- [x] Probar anotaciones y sus streams de apariencia.
- [ ] Probar formularios, campos homónimos y widgets.
- [x] Probar marcadores y destinos con nombre según la política acordada.
- [ ] Probar fuentes cifradas y permisos insuficientes.
- [x] Probar documentos principales cifrados con credenciales completas e
  incompletas.
- [x] Probar advertencias de firma para destino y fuentes.
- [x] Probar que los errores no dejan mutaciones parciales.
- [x] Mantener determinismo con reloj e identificadores inyectados.

### Auditoría de cobertura (2026-09-10)

Estos ítems no se implementaron ahora: ya estaban cubiertos y nadie los había
tildado. Cada tilde de arriba apunta a un test que existe hoy y falla si la
garantía se rompe. Los cuatro que siguen abiertos lo están por un motivo
concreto, no por olvido.

| Ítem | Dónde está probado |
|---|---|
| Texto, imágenes y recursos anidados | `pdf-manip/tests/graft.rs`: `graft_pages_copies_resources_nested_inside_stream_dictionaries` recorre página → XObject → imagen → `/SMask`, y imagen → ColorSpace indexado → tabla de consulta; `graft_pages_copies_the_object_graph_the_page_reaches` cubre la fuente a dos saltos |
| Colisiones de identificadores | `graft_pages_survives_overlapping_object_ids_between_the_two_documents` (dos fixtures construidas igual, ids solapados exactamente); más `import_pages_rejects_an_id_already_in_the_document` e `import_pages_rejects_duplicate_ids_within_the_batch` en `edit_log` |
| Enlaces entre páginas importadas | `pdf-manip/tests/graft_destinations.rs` entero (8 casos): destino explícito dentro de la selección, destino con nombre reescrito a explícito, `/Dests` heredado, y los tres casos de destino fuera de la selección que se reportan en vez de romperse |
| Marcadores y destinos con nombre | `graft_destinations.rs` para destinos; `graft_structures.rs` para marcadores (`only_the_bookmarks_pointing_into_the_selection_are_reported`, `importing_every_page_reports_every_bookmark`, `an_ordinary_import_reports_nothing_at_all`) |
| Documento principal cifrado, credenciales completas e incompletas | `pdf-manip/tests/encrypted_open.rs`: contraseña de usuario correcta, de propietario correcta, ambas conservando el contrato de cifrado, contraseña incorrecta rechazada sin pánico, y contraseña ausente rechazada |
| Errores sin mutaciones parciales | `edit_log`: `an_empty_import_changes_neither_history_nor_redo`, `removing_a_mismatched_imported_batch_is_rejected_before_mutating`, `an_out_of_range_import_is_rejected_without_losing_redo`, `invalid_move_pages_commands_are_rejected_before_mutating`, `rejected_move_pages_preserves_undo_and_redo_history`. En el guardado: `a_refused_import_writes_nothing_at_all` y la guarda de `replay_page_ops` que rechaza antes de copiar el primer objeto |
| Anotaciones y sus streams de apariencia | `graft_pages_carries_an_annotations_appearance_streams` recorre página → `/Annots` → `/AP` → estados `/N` y `/R` (ambos), y desde el stream normal → `/Resources` → `/Font` → la fuente con la que dibuja |
| Determinismo con reloj e ids inyectados | `strategy::tests::full_rewrite_with_fixed_options_is_byte_identical_across_runs` compara dos guardados completos; `clock.rs` prueba `FixedClock` y `SequentialIdGenerator` por separado, incluido `two_sequential_generators_with_same_seed_produce_identical_sequences` |

Lo que sigue abierto, y por qué:

- **Atributos heredados y árboles de páginas anidados** — la mitad heredada
  está probada: `pdf_with_inherited_attributes` deja `/MediaBox`,
  `/Resources` y `/Rotate` solo en la raíz del árbol y
  `graft_pages_materializes_attributes_the_source_page_only_inherited`
  comprueba que el injerto los materializa. Lo que falta es el árbol
  **anidado**: todas las fixtures construyen un único nodo `Pages` con
  páginas colgando; ninguna mete un `Pages` dentro de otro `Pages`, que es
  donde la herencia recorre más de un salto y donde un `/Parent` mal
  reescrito no se notaría.
- **Formularios, campos homónimos y widgets** — no se puede probar todavía.
  La política de §4 es rechazar una página con widgets AcroForm, y eso sí
  está probado (`graft_pages_rejects_a_selected_page_with_a_form_field_widget`,
  `graft_pages_allows_a_source_whose_unselected_page_has_form_fields`). Los
  campos homónimos solo existen como problema cuando la fusión exista; este
  ítem se cierra junto con los dos abiertos de §4, no antes.
- **Fuentes cifradas y permisos insuficientes** — el predicado sí está
  probado en `pdf-manip/src/security.rs`
  (`an_unencrypted_document_permits_assembly`,
  `the_modify_contents_bit_alone_permits_assembly`,
  `neither_the_annotate_nor_the_copy_bit_permits_assembly`), pero el recorrido
  de una *fuente* cifrada o sin permiso de copiar/extraer vive hoy en el shell
  Linux, no en el núcleo. Cerrarlo en §12 exige mover esa comprobación al
  núcleo o aceptar que el ítem pertenece a §13; conviene decidirlo antes de
  escribir el test.

## 13. Pruebas GTK4

- [ ] Probar disponibilidad y estado del botón de importación.
- [ ] Probar selección múltiple y cancelación.
- [x] Probar el selector `Documents | Pages`.
- [x] Probar que el selector no crea comandos ni marca cambios.
- [x] Probar tarjetas, nombres, recuentos, rangos y etiquetas de partes.
- [x] Probar arrastre de bloques como una operación atómica.
- [x] Probar mezcla de páginas y reconstrucción de tramos.
- [x] Probar que el orden permanece intacto al volver a documentos.
- [x] Probar destinos de arrastre en huecos y al final.
- [x] Probar alternativas de teclado y etiquetas accesibles.
- [x] Probar tipo y duración configurada de la transición sin depender de
  temporizadores reales.
- [x] Probar el comportamiento con animaciones desactivadas.
- [ ] Probar que resultados asíncronos obsoletos se descartan. Parcial: hay
  test de la condición que los descarta (la generación de la caché de
  miniaturas), no del descarte mismo, que ocurre dentro del futuro de render
  y necesita pdfium.
- [ ] Probar que una importación fallida conserva la sesión anterior.

## 14. Gates de verificación

- [ ] Ejecutar `cargo fmt --all -- --check`.
- [ ] Ejecutar `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- [ ] Ejecutar `cargo build --workspace --locked`.
- [ ] Ejecutar `cargo test --workspace --locked -- --skip gtk_ui_`.
- [ ] Ejecutar la suite GTK4 bajo Xvfb en Linux.
- [ ] Ejecutar el empaquetado y smoke test de Linux.
- [ ] Ejecutar `python3 scripts/check_maintainability.py`.
- [ ] Registrar comandos, resultados y gates no disponibles en la entrega.

## Criterios de cierre

- [ ] Se pueden añadir varios PDFs al documento abierto desde Linux.
- [ ] Se pueden ordenar como documentos completos o como páginas individuales.
- [ ] Los cambios entre vistas nunca alteran el orden.
- [ ] Los tramos separados del mismo PDF aparecen como bloques separados.
- [ ] El contenido importado conserva su naturaleza PDF y no se rasteriza.
- [ ] Importaciones y movimientos de bloque tienen deshacer y rehacer atómicos.
- [ ] Guardar y reabrir conserva contenido, orden y estructuras soportadas.
- [ ] Los casos no soportados se rechazan antes de perder información.
- [ ] La interfaz permanece fluida con documentos grandes.
- [ ] Todos los gates disponibles están ejecutados y documentados.

## Fuera de alcance inicial

- [ ] Implementación de la interfaz equivalente en Windows.
- [ ] Implementación de la interfaz equivalente en macOS.
- [ ] Implementación de la interfaz equivalente en Android.
- [ ] Implementación de la interfaz equivalente en iOS.
- [ ] Persistencia de la procedencia original mediante metadatos personalizados
  después de cerrar y volver a abrir el PDF resultante.
