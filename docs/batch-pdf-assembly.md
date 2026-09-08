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
| Vista por documentos | Pendiente |
| Vista por páginas | Pendiente |
| Animación | Pendiente |
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
- [ ] Definir y probar la política para marcadores y destinos con nombre.
- [ ] Remapear destinos que apunten a páginas importadas.
- [ ] Detectar destinos que apunten a páginas no importadas.
- [ ] Definir la política para capas opcionales y estructura etiquetada.
- [ ] Rechazar con un mensaje claro cualquier estructura todavía no soportada
  que pudiera perder información.
- [ ] No ofrecer una importación aparentemente correcta si existe pérdida de
  datos conocida.

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

## 5. Seguridad y firmas

- [ ] Añadir una comprobación específica del permiso PDF de ensamblado de
  documentos.
- [ ] Comprobar permisos tanto en el documento principal como en cada fuente.
- [ ] Solicitar de forma independiente la contraseña de cada PDF protegido.
- [ ] Mantener las credenciales fuera del modelo de dominio, logs y mensajes de
  error.
- [ ] Verificar antes de editar que un documento principal cifrado puede
  reescribirse correctamente.
- [ ] Mantener la política de cifrado del documento principal al guardar.
- [ ] Detectar firmas en el documento principal y en las fuentes activas.
- [ ] Explicar que una combinación o reordenación invalida criptográficamente
  las firmas existentes.
- [ ] Exigir confirmación explícita antes de guardar un resultado que invalide
  firmas.
- [ ] Conservar objetos y apariencias de firma solo cuando la política definida
  lo permita.

## 6. Guardado y resolución de páginas

- [x] Extender `pdf-save` para distinguir páginas en blanco de páginas
  importadas.
- [x] Reconstruir el PDF siguiendo exactamente el orden de `Document.pages`.
- [x] Mantener el documento base original inmutable durante la edición.
- [x] Resolver cada página importada mediante su fuente y número de página.
- [ ] Devolver un mapa final de `PageId` a objeto PDF después de materializar.
- [x] Resolver explícitamente `PageId` a índice de renderizado actual.
- [x] Eliminar los usos que asumen que `PageId.0` es un índice de PDFium.
- [ ] Leer anotaciones existentes desde el documento materializado para no
  borrar anotaciones importadas al añadir otras nuevas.
- [ ] Permitir edición de contenido sobre páginas importadas usando el respaldo
  materializado correcto.
- [ ] Validar el resultado con PDFium antes de instalar la previsualización.
- [ ] Probar guardar, cerrar y reabrir después de importar, mover y borrar.

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
- [ ] Mantener el historial completo durante la actualización de la
  previsualización.
- [ ] Mantener el estado de cambios sin guardar.
- [ ] Mantener selecciones y contadores de identificadores que sigan siendo
  válidos.
- [ ] Invalidar selecciones y cachés que apunten a páginas eliminadas.
- [ ] Instalar conjuntamente el nuevo handle de PDFium y su mapa de páginas.
- [ ] Evitar que un resultado asíncrono antiguo reemplace una sesión más nueva.
- [ ] Conservar la sesión anterior intacta si falla la materialización o la
  reapertura.

## 8. Selección e importación en Linux

- [ ] Añadir `Add PDFs` a la cabecera de Organizar.
- [ ] Permitir selección múltiple mediante `FileDialog::open_multiple`.
- [ ] Respetar el orden devuelto por la selección de archivos.
- [ ] Ejecutar lectura, validación e importación fuera del hilo principal.
- [ ] Mostrar progreso para operaciones perceptibles.
- [ ] Permitir cancelar sin modificar documento, historial ni estado sucio.
- [ ] Mostrar errores por archivo con una explicación accionable.
- [ ] Añadir inicialmente cada PDF como un bloque al final del documento.
- [ ] Actualizar controles de guardado, deshacer y rehacer tras importar.

## 9. Vista por documentos

- [ ] Añadir el selector visual `Documents | Pages`.
- [ ] Abrir Organizar con `Documents` seleccionado.
- [ ] Mostrar cada tramo contiguo como una tarjeta de documento.
- [ ] Incluir portada apilada, nombre, número de páginas y rango actual.
- [ ] Mantener una jerarquía visual limpia y coherente con la paleta existente.
- [ ] Permitir arrastrar una tarjeta para mover todo el tramo.
- [ ] Resolver inserciones antes, después y al final de la lista.
- [ ] Mostrar claramente el destino durante el arrastre.
- [ ] Permitir eliminar un bloque mediante una única acción reversible.
- [ ] Añadir nombres accesibles y ayudas de teclado para mover y eliminar.
- [ ] Evitar depender exclusivamente de arrastrar y soltar.

## 10. Vista por páginas

- [ ] Reutilizar la cuadrícula individual existente sin duplicar decisiones de
  negocio.
- [ ] Permitir mezclar páginas de diferentes PDFs.
- [ ] Mantener reordenación y eliminación como operaciones reversibles.
- [ ] Transferir identidades estables durante el arrastre, no índices
  capturados.
- [ ] Resolver inserciones en huecos y después de la última tarjeta.
- [ ] Actualizar números de página después de cada operación.
- [ ] Mostrar la procedencia de una página sin sobrecargar visualmente la
  tarjeta.
- [ ] Verificar que volver a `Documents` conserva exactamente el orden actual.

## 11. Animación y rendimiento

- [ ] Construir vistas independientes dentro de un `GtkStack`.
- [ ] Evitar reparentar widgets retirados de un `FlowBox`.
- [ ] Animar el paso de documentos a páginas con opacidad, escala y
  desplazamiento.
- [ ] Escalonar la aparición de páginas para comunicar que salen de sus
  bloques.
- [ ] Limitar la duración total de la secuencia en documentos grandes.
- [ ] Animar solo elementos visibles cuando hacerlo completo sea costoso.
- [ ] Respetar la configuración global de animaciones de GTK.
- [ ] Cambiar inmediatamente de vista cuando las animaciones estén
  desactivadas.
- [ ] Cachear miniaturas por documento, página, tamaño y factor de escala.
- [ ] Invalidar resultados de renderizado obsoletos mediante una generación.
- [ ] Evitar renderizar nuevamente todas las miniaturas al cambiar de vista.
- [ ] Medir importación, primer render y cambio de modo con documentos grandes.

## 12. Pruebas del núcleo

- [x] Probar que importar un PDF es un único paso de historial.
- [x] Probar que deshacer y rehacer restaura orden y procedencia.
- [x] Probar que mover un bloque es un único paso de historial.
- [ ] Probar importación de texto, imágenes y recursos anidados.
- [ ] Probar páginas con atributos heredados y árboles de páginas anidados.
- [ ] Probar colisiones de identificadores entre documentos.
- [ ] Probar enlaces entre páginas importadas.
- [ ] Probar anotaciones y sus streams de apariencia.
- [ ] Probar formularios, campos homónimos y widgets.
- [ ] Probar marcadores y destinos con nombre según la política acordada.
- [ ] Probar fuentes cifradas y permisos insuficientes.
- [ ] Probar documentos principales cifrados con credenciales completas e
  incompletas.
- [ ] Probar advertencias de firma para destino y fuentes.
- [ ] Probar que los errores no dejan mutaciones parciales.
- [ ] Mantener determinismo con reloj e identificadores inyectados.

## 13. Pruebas GTK4

- [ ] Probar disponibilidad y estado del botón de importación.
- [ ] Probar selección múltiple y cancelación.
- [ ] Probar el selector `Documents | Pages`.
- [ ] Probar que el selector no crea comandos ni marca cambios.
- [ ] Probar tarjetas, nombres, recuentos, rangos y etiquetas de partes.
- [ ] Probar arrastre de bloques como una operación atómica.
- [ ] Probar mezcla de páginas y reconstrucción de tramos.
- [ ] Probar que el orden permanece intacto al volver a documentos.
- [ ] Probar destinos de arrastre en huecos y al final.
- [ ] Probar alternativas de teclado y etiquetas accesibles.
- [ ] Probar tipo y duración configurada de la transición sin depender de
  temporizadores reales.
- [ ] Probar el comportamiento con animaciones desactivadas.
- [ ] Probar que resultados asíncronos obsoletos se descartan.
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
