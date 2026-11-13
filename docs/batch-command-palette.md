# Ficha de batch B27 — Paleta de comandos (Ctrl+K) en Linux

> Documentado 2026-11-13, a pedido explícito del usuario. Mismo formato de ficha que
> B24 (compresión) y B26 (web). **No hay código todavía.**
>
> Origen del scope: en el roadmap de funciones de Vitela, "Ctrl+K para buscar comandos —
> acceso rápido a cualquier función" quedó con prioridad Alta. Se arranca por el shell
> GTK4 porque es el más completo; los demás shells quedan fuera de esta ficha.

**Dependencias:** ninguna del núcleo. Es trabajo solo de `apps/linux-gtk`.
Numeración: siguiente libre tras B26. Tareas: T-228 en adelante (T-227 es la última
usada, en `batch-web.md`). **Re-verificar que siguen libres al arrancar**: hay ramas
web abiertas que consumen números de T-.

## Hechos clave del shell

Rutas relativas a `apps/linux-gtk/src/app/`, medidas sobre `origin/main` el 2026-11-13.

1. **Ctrl+K está libre.** Los atajos de ventana se registran todos con
   `set_accels_for_action`: Ctrl+O/S/P/F/N en `connect_standard_shortcuts`
   (`mod.rs:601`), Ctrl+Z/Y en `annotations/command.rs:35`, Ctrl+C en
   `selection.rs:1610`, Ctrl+V en `input.rs:48`, Ctrl+X en
   `content_edit/clipboard.rs:310`, Supr, F2 y Esc en sus módulos. No hay
   `ShortcutController` ni `GtkShortcutsWindow`.

2. **Casi nada es una `gio::SimpleAction`.** Solo existen `win.open`, `win.save`,
   `win.print`, `win.find`, `win.new`, `win.undo`, `win.redo`, `win.copy`,
   `win.paste`, `win.delete-annotation` y las tres constantes de `content_edit`
   (retype, cut, clear). Modos, exportar, comprimir, proteger, firmar y organizar son
   handlers `connect_clicked`. No hay acciones `app.`.

3. **El punto de entrada de herramientas ya existe.** `home::tools::apply`
   (`home/tools.rs:251`) se documenta como "la única definición de lo que significa cada
   herramienta", y lo comparten el grid de Home y la barra de modos. `open_tool`
   (`home/tools.rs:228`) cubre el arranque en frío: sin documento abierto guarda
   `pending_tool` y abre el selector de archivos. `HomeTool` (`state.rs:437`) y `Mode`
   (`mode_bar.rs:86`, con `ALL`, `label_key`, `icon`, `tool()`) ya son enums cerrados.
   Las herramientas de anotación tienen `Tool::ALL` (`state.rs:535`); los campos de
   formulario, `FieldKind::ALL`.

4. **No hay una función única de disponibilidad.** Cada feature tiene su
   `update_*_controls`. Lo que sí está centralizado son los rechazos por permisos `/P`:
   métodos de `Viewer` que devuelven `Option<String>` con el motivo
   (`text_extraction_refusal`, `annotation_editing_refusal`, `content_edit_refusal`,
   `page_assembly_refusal`, `full_rewrite_refusal`, `state.rs:173-255`), más
   `signing_refusal`, `protection_refusal`, `import_refusal` y
   `structural_edit_refusal` en sus módulos. "Hay documento" es
   `viewer.state.borrow().session.is_some()`.

5. **i18n con Fluent, inglés y español** (`locales/en.ftl`, `locales/es.ftl`, `tr` /
   `tr_args` en `i18n/mod.rs:66`). Una clave faltante entra en pánico en debug. Ya hay
   claves reutilizables: `mode-*`, `annotate-tool-*`, `home-quick-*`, `home-shortcut-*`,
   `editor-find*`. No existen claves de sinónimos.

6. **Ya hay un filtro de herramientas, y no es difuso.** El `SearchEntry` de Home
   (`home/header.rs:36`) filtra recientes y tiles por subcadena en minúsculas
   (`home/mod.rs:272-281`). La búsqueda en el documento (Ctrl+F, `search.rs`) es exacta y
   no sirve para esto. No hay código de coincidencia difusa en el shell.

7. **La lista de atajos visible está escrita a mano.** `build_shortcuts_card`
   (`home/tools.rs:369`) muestra cinco atajos con el texto literal "Ctrl+O". El atajo
   nuevo tiene que aparecer ahí.

8. **No hay enum de pantallas.** `viewer.view_stack` tiene las páginas `home`,
   `editor` y `organize`; `ModeBar::sync` deduce el modo actual de la página visible.

## Decisiones de diseño

1. **Módulo nuevo `app/palette/`, partido desde el día uno** (regla de no-monolitos de
   `CLAUDE.md`): `matcher.rs` (puro, sin GTK), `entries.rs` (el catálogo), `view.rs`
   (widgets) y `mod.rs` (acción y cableado). El tipo se llama `PaletteEntry`, **no**
   `Command`: `Command` ya es el tipo de edición del núcleo (`Command::MovePage`, etc.)
   y una búsqueda de "Command" no debe devolver dos cosas distintas.

2. **La paleta no tiene caminos propios: llama a los que ya existen.** Una entrada
   activa una acción `win.*` si existe, o llama a `home::tools::apply` / `open_tool`, o a
   la misma función que hoy invoca el botón. Si un botón hace algo que ninguna función
   expone, la tarea extrae esa función y el botón pasa a llamarla también. Así un comando
   desde la paleta y un clic no pueden divergir.
   *Alternativa descartada:* migrar todo el shell a `gio::SimpleAction` primero. Es lo
   más limpio a largo plazo, pero son decenas de handlers y no lo necesita la v1. Queda
   como deuda anotada, no como prerequisito.

3. **Las entradas no disponibles se muestran deshabilitadas, con el motivo.** No se
   ocultan. El motivo es el texto que ya devuelven los `*_refusal` del hecho 4, sin
   redactar uno nuevo. Esconderlas haría que "no aparece" se confunda con "no existe".
   Excepción: sin documento abierto, las herramientas de `HomeTool` siguen disponibles,
   porque `open_tool` ya resuelve ese caso pidiendo el archivo.

4. **Coincidencia difusa propia, pura y testeada sin GTK.** Subsecuencia con puntaje:
   gana el inicio de palabra, gana la coincidencia contigua y gana el prefijo. Se ignoran
   mayúsculas y **tildes** ("pagina" encuentra "Página"). Se busca en la etiqueta en el
   idioma activo, en la etiqueta en inglés y en una lista de sinónimos por entrada
   (`palette-*-keywords` en los `.ftl`). Sin dependencias nuevas: es una función de
   unas decenas de líneas y una crate de fuzzy matching no se justifica.

5. **La paleta es un `gtk::Overlay` sobre la ventana principal, no una ventana nueva.**
   Un `SearchEntry` arriba y una `ListView` debajo, centrados en la parte superior. Así
   no hay foco que cruce ventanas y se evita la trampa de mapeo de diálogos de WSLg. Esc
   o un clic afuera cierran, las flechas mueven la selección, Enter ejecuta. Al cerrar, el
   foco vuelve al widget que lo tenía, sin pasar por el fallback de GTK (la trampa de
   "foco que salta y scrollea el canvas" ya documentada para el editor de contenido).

6. **Ctrl+K es `win.command-palette`, registrado en `connect_standard_shortcuts`.**
   Funciona en Home, editor y Organize. Mientras un cuadro de texto del editor de
   contenido o de una anotación tiene el foco, **hay que medir** si `GtkText` consume
   Ctrl+K antes que la acción de ventana. Si lo consume, se deja así: dentro de un cuadro
   de texto el teclado es del texto.

7. **El atajo de cada entrada sale de la misma tabla que lo registra.** La tabla de
   `connect_standard_shortcuts` pasa a ser una constante compartida, y la paleta muestra
   el atajo leyendo esa constante. La tarjeta de atajos de Home (hecho 7) también. Un
   atajo escrito en tres lugares termina desincronizado.

8. **Con la búsqueda vacía se listan los comandos usados en la sesión, después el resto
   por categoría.** Los recientes viven en memoria y no se guardan en disco en la v1.

## Límites de responsabilidad

- `palette/matcher.rs` posee el puntaje y la normalización (minúsculas, tildes). No
  conoce GTK ni el catálogo.
- `palette/entries.rs` posee qué comandos existen, su etiqueta, sinónimos, atajo,
  disponibilidad y a qué función llaman. No decide permisos: consulta los `*_refusal`.
- Cada feature sigue siendo dueña de su comportamiento. La paleta solo lo invoca.
- El núcleo no se toca.

## Fases y tareas

### Fase 1 — Lógica pura
- [x] T-228 `palette/matcher.rs`: normalización (minúsculas, tildes) y puntaje por
      subsecuencia. TDD estricto: casos de inicio de palabra, contiguo contra disperso,
      sin coincidencia, empate estable por orden del catálogo y "pagina" contra
      "Página". [Palette]
      **(2026-11-13 — completo.** `app/palette/mod.rs` solo declara `mod matcher;`;
      `mod palette;` queda registrado en `app/mod.rs`. API: `normalize(&str) -> String`,
      `score(query, candidate) -> Option<u32>` y `rank<T>(query, &[T], impl Fn(&T) ->
      Vec<String>) -> Vec<(usize, u32)>`. `rank` toma el mejor puntaje entre los textos
      de cada entrada (etiqueta activa, etiqueta en inglés, sinónimos), descarta las que
      no coinciden y ordena por puntaje descendente con desempate por índice del
      catálogo. Los textos son `String` porque las etiquetas traducidas salen de `tr`
      como valores propios.
      **Decisiones que la ficha no fijaba:** consulta vacía o solo espacios →
      `Some(0)` para todo, así `rank` devuelve el catálogo en su orden (base de la
      decisión 8). La consulta se parte por espacios y **cada término** tiene que
      aparecer como subsecuencia; los términos se buscan por separado, en cualquier
      orden y pueden solaparse ("pagina rotar" también encuentra "Rotar página"); el
      puntaje es la suma. Puntos por carácter: 1, +8 inicio de palabra (comienzo, o
      tras cualquier espacio en blanco o `-`, `_`, `/`, `(`, `)`, `.`, `,`, `:`), +4
      contiguo, +12 si el término arranca en el primer carácter. Cada término prueba
      todas las posiciones de su primer carácter; cada carácter siguiente va, en este
      orden: justo después del anterior (una corrida contigua como "rot" en "Rotate" no
      se rompe), o en la primera aparición posterior que sea inicio de palabra si el
      resto del término todavía cabe después de ella, o en la primera aparición. No es
      una búsqueda exhaustiva de alineaciones, pero "ep" en "Export pages" pone la `p`
      en "pages" y no dentro de "Export". **Corrección de la revisión de T-228**
      (aplicada en T-229): la primera versión solo buscaba inicio de palabra para el
      primer carácter de cada término y tomaba la aparición más temprana para el
      resto, y sus separadores de palabra eran solo espacio, `-`, `_` y `/`; esta nota
      y el doc comment ya describen el código corregido. La tabla de tildes es explícita, sin
      crate: minúsculas de Latin-1 Supplement y Latin Extended-A (español, portugués,
      francés, alemán), más `ß`→"ss", `æ`→"ae", `œ`→"oe"; el resto (griego, cirílico,
      CJK, marcas combinantes) pasa solo en minúsculas. El `dead_code` se silenciaba
      con un único `#![cfg_attr(not(test), allow(dead_code))]` en `matcher.rs`; T-229
      lo quitó al llamar a `rank`, y `score` (la forma de un solo candidato) quedó
      `#[cfg(test)]` porque solo la usan los tests.
      **Evidencia TDD:** 16 tests escritos primero contra cuerpos `todo!()`: 0 pasan /
      16 fallan; tras implementar, 16/16. Gate en WSL: `cargo fmt --all -- --check`,
      `cargo clippy -p linux-gtk --all-targets --locked -- -D warnings` y
      `cargo test -p linux-gtk --locked palette::` en verde; la suite completa de
      `linux-gtk`: **974 pasan, 0 fallan, 1 ignorado**.)
- [x] T-229 `palette/entries.rs`: `PaletteEntry { id, label_key, keywords_key,
      shortcut, category, availability, run }` y el catálogo de la v1:
      - modos de `Mode::ALL` (vía `apply` / `open_tool`);
      - las acciones `win.*` del hecho 2;
      - herramientas de anotación de `Tool::ALL` y de formulario de `FieldKind::ALL`;
      - las herramientas que hoy solo son `connect_clicked` (exportar imágenes,
        comprimir, proteger, firmar, marca de agua, organizar, rotar, añadir PDFs). La
        tarea **enumera la lista real al arrancar** y extrae la función de cada botón
        según la decisión 2.
      Un test fija que todo `Mode` y todo `Tool` tiene su entrada, para que una
      herramienta nueva no quede fuera de la paleta en silencio. [Palette]
      **(2026-11-13 — completo.** Partido en `palette/entries/mod.rs` (tipos, catálogo,
      `search`), `entries/invoke.rs` (qué corre cada entrada y su disponibilidad) y
      `entries/tests.rs`. `PaletteEntry { id, label_key, keywords_key, shortcut,
      category, target }`: en vez de dos punteros `fn` (`availability`, `run`), un
      `Target` cerrado (`Mode`, `Action(Shortcut)`, `Annotation(Tool)`,
      `FormField(FieldKind)`, `Click(ClickTool)`) y dos métodos que hacen `match`
      sobre él; un `fn` no puede capturar qué `Mode` o qué `Tool` es, y el `match`
      obliga a cubrir cada variante nueva. `run` recibe `&ApplicationWindow` además
      del `&Viewer` porque `open_tool` y los selectores de archivos lo necesitan.
      `Availability` es `Available | Refused(String)`. `search(viewer, query)`
      devuelve `(PaletteEntry, Availability, u32)` con `matcher::rank` sobre la
      etiqueta activa, la etiqueta en inglés y los sinónimos; con la consulta vacía,
      el orden del catálogo (los recientes de la decisión 8 quedan para T-230).
      **Catálogo real: 35 entradas.** Modos (6, uno por `Mode::ALL`, vía
      `Mode::open`); archivo (4: abrir, nuevo, guardar, imprimir) y edición (3:
      deshacer, rehacer, buscar), todas `win.*` activadas en la ventana; anotar (8,
      `Tool::ALL`); formularios (4, `FieldKind::ALL`); documento (3: exportar
      imágenes, marca de agua, comprimir); páginas (3: añadir PDFs, extraer,
      dividir); firma (4: certificado `.pfx`, tarjeta o token, certificado del equipo,
      dibujar firma). Etiquetas: todas claves existentes (`mode-*`, `home-quick-*`,
      `editor-*`, `annotate-tool-*`, `forms-kind-*`, `edit-watermark`,
      `home-tool-compress`, `organize-*`, `sign-*`, `signature-draw`).
      `keywords_key` es `None` en todas (T-231).
      **Lista real de herramientas solo-clic** (enumerada al arrancar): exportar
      imágenes (`export::begin_export`), marca de agua
      (`content_edit::watermark::begin_watermark`), añadir PDFs
      (`organize::import::show_chooser`), extraer y dividir
      (`write::begin_extract`/`begin_split`), los tres botones de certificado
      (`sign::show_pfx_chooser`, `begin_pkcs11_flow`, `begin_nss_flow`) y dibujar firma
      (`signature::flow::open`). Comprimir y proteger ya pasaban por
      `home::tools::apply` (`HomeTool`); comprimir entra por `open_tool` como su
      tarjeta de Home, y organizar y proteger son modos. **Ningún botón tenía lógica
      dentro del closure**: todos llamaban ya a una función con nombre, así que la
      única extracción fue `Mode::open` en `mode_bar.rs`, el `match` que el botón de
      modo tenía en su closure; ahora el botón y la paleta llaman a esa función. El
      resto solo cambió de visibilidad: `structural_edit_refusal`, `signing_refusal`,
      `protection_refusal`, `import_refusal`, `show_chooser`, los tres flujos de firma
      y `flow::open` (reexportado como `signature::draw_signature`, que el botón
      también usa). `Mode::label_key` pasó a `pub(crate)`; `Tool` y `FieldKind`
      ganaron `label_key()` y su `label()` la usa.
      **Fuera, y por qué:** **rotar.** Sus botones giran la página o el bloque de una
      tarjeta de Organize; no hay rotación a nivel de documento a la que llamar, e
      inventar "girar la página actual" sería un camino propio (decisión 2). El
      criterio de aceptación "rotar pagina" queda pendiente de esa decisión.
      **Decisiones que la ficha no fijaba:** (1) Las anotaciones y los campos de
      formulario se ejecutan llevando el editor a su página (Anotar / Rellenar y
      firmar) y haciendo `set_active(true)` en su toggle, que dispara el mismo
      handler que el clic; si ya estaba activo, sigue activo. (2) Las cuatro entradas
      de firma abren primero la página Rellenar y firmar: la firma dibujada se suelta
      al salir de esa página (`signature::connect_signature`). (3) Añadir PDFs abre
      antes Organize si no está en pantalla, porque el progreso y el cancelar de la
      importación viven en su cabecera; si `organize::show` lo rechaza, no se abre el
      selector. (4) Disponibilidad: sin documento, los modos y comprimir siguen
      disponibles (`open_tool` pide el archivo), abrir y nuevo también; el resto se
      rechaza con el texto que la propia función muestra sin documento
      (`save-no-document`, `print-open-first`, `search-open-first`,
      `annotate-no-document`, `forms-no-document`, `export-no-document`, etc.). Con
      documento: Organizar → `content_edit_refusal`, Proteger →
      `protection_refusal`, buscar y exportar → `text_extraction_refusal`, marca de
      agua → `content_edit_refusal`, añadir PDFs → `import_refusal`, extraer/dividir →
      extracción y luego `full_rewrite_refusal` (el orden de `begin_extract`), firma
      con certificado → `signing_refusal`, dibujar → `annotation_editing_refusal`,
      campos → `structural_edit_refusal`; Editar, Anotar y Firmar como modos son
      navegación y no se rechazan. Comprimir no muestra rechazo: necesita la
      instantánea de guardado completa y `begin_compress` lo informa al ejecutarse.
      Deshacer/rehacer siguen `is_enabled()` de su `SimpleAction`; para ese caso no
      había texto y se añadieron **dos claves** en `en.ftl` y `es.ftl`:
      `palette-nothing-to-undo`, `palette-nothing-to-redo`. Buscar activa `win.find`,
      que solo abre el popover si el editor está en pantalla; T-230 decide si la
      paleta lleva antes al editor. (5) Decisión 7, mínimo: nuevo `app/shortcuts.rs`
      con un `Shortcut { action, accel }` por acción (`OPEN`, `SAVE`, `PRINT`, `FIND`,
      `NEW`, `UNDO`, `REDO`); `connect_standard_shortcuts`,
      `connect_history_shortcuts`, los `SimpleAction::new` de las siete acciones y el
      catálogo leen esas constantes. La tarjeta de atajos de Home sigue igual (T-232).
      (6) `i18n::tr_in(Language, key)` para la etiqueta en inglés, con los mismos
      fallbacks que `tr`, y `has_message(Language, key)` solo para tests. (7) La
      búsqueda en español sin tildes se prueba con "anadir pdf" e "imagenes": ninguna
      etiqueta del catálogo contiene "página", así que "pagina" no encuentra nada
      hasta que T-231 escriba los sinónimos. `entries` lleva
      `#![cfg_attr(not(test), allow(dead_code))]` hasta que T-230 lo llame.
      **Evidencia TDD:** corrección del matcher: 4 tests nuevos, 2 en rojo contra el
      código de T-228 ("ep" en "Export pages" y la `z` tras `(`; los otros 2 fijan que
      una corrida contigua no se rompe y que un inicio de palabra que deja sin sitio al
      resto se salta), 20/20 tras corregir. Catálogo: 18 tests; contra `invoke.rs` con
      cuerpos `todo!()`, 8 pasan (los de forma del catálogo, que es dato) y 10 fallan;
      tras implementar, 18/18 (filtro `palette::`: 28/38 → 38/38). Más 3 tests de
      `shortcuts` y 1 de `i18n::tr_in`. Gate en WSL: `cargo fmt --all -- --check`,
      `cargo clippy -p linux-gtk --all-targets --locked -- -D warnings` y
      `cargo test -p linux-gtk --locked` en verde: **1000 pasan, 0 fallan, 1
      ignorado**.)

### Fase 2 — UI
- [ ] T-230 `palette/view.rs` + `mod.rs`: overlay, `SearchEntry`, `ListView` con
      etiqueta, atajo y motivo de deshabilitado; navegación por teclado; ejecutar y
      cerrar; devolver el foco. Acción `win.command-palette` con Ctrl+K. [PaletteUI]
- [ ] T-231 Claves Fluent en `en.ftl` y `es.ftl`: etiquetas que falten, sinónimos
      `palette-*-keywords` y el placeholder. Se reutilizan las claves existentes del
      hecho 5 en vez de duplicarlas. [PaletteUI]
- [ ] T-232 Tabla de atajos compartida (decisión 7): `connect_standard_shortcuts`, la
      paleta y `build_shortcuts_card` leen la misma constante. La tarjeta de Home suma
      Ctrl+K. [PaletteUI]

### Fase 3 — Pruebas y docs
- [ ] T-233 Tests `gtk_ui_palette_*` sobre `ui_tests::built_ui()`: Ctrl+K abre en las
      tres pantallas; Esc cierra y el foco vuelve; Enter ejecuta un modo; una entrada
      rechazada por `/P` aparece deshabilitada con el motivo y Enter no hace nada; una
      búsqueda en español sin tildes encuentra su comando. Más la medición de la
      decisión 6. [Palette]
- [ ] T-234 README: fila de la paleta en la tabla de Linux. [Docs]

## Criterios de aceptación

- Ctrl+K abre la paleta desde Home, el editor y Organize, con el foco en el campo.
- Cada modo de la barra de modos y cada herramienta de anotación se puede ejecutar desde
  la paleta, y el resultado es el mismo que hacer clic en su botón.
- Un comando no disponible se ve deshabilitado y muestra el mismo motivo que su botón.
- "rotar pagina", escrito sin tilde, encuentra el comando de rotar en español; "rotate"
  lo encuentra en cualquier idioma.
- Esc cierra la paleta sin cambiar nada y el canvas no se desplaza.
- La tarjeta de atajos de Home y la paleta muestran el mismo texto de atajo que el
  registrado en GTK.

## Fuera de scope (v1)

Windows, Android, macOS y web: cada shell tendrá su ficha o su tarea cuando esta cierre
· Buscar dentro del documento o en archivos recientes desde la paleta (Ctrl+F y el
buscador de Home ya lo hacen) · Atajos configurables por el usuario · Guardar los
recientes en disco · Migrar todo el shell a `gio::SimpleAction` (decisión 2).

## Orden de ejecución

Fase 1 → 2 → 3, TDD estricto. T-228 primero y solo, porque es la única pieza que se
puede probar entera sin GTK. T-229 antes que la UI, para que el overlay nazca leyendo un
catálogo real.
