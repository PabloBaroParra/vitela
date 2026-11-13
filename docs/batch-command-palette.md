# Ficha de batch B27 — Paleta de comandos (Ctrl+K) en Linux

> Documentado 2026-11-13, a pedido explícito del usuario. Mismo formato de ficha que
> B24 (compresión) y B26 (web). **T-228…T-234 cerradas (#578–#583 y el PR de cierre).** Queda abierto
> un criterio de aceptación: la revisión visual en la app (ver "Criterios de
> aceptación"). "rotar pagina" se cerró después con dos entradas de rotar todas las
> páginas (ver "Añadido posterior — rotar todas las páginas", tras T-234).
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
      **Corrección posterior (añadido rotar todas las páginas, 2026-11-13).** Bonus de
      corrida entera: un término que coincide como palabra entera o prefijo de palabra
      (empieza en inicio de palabra y todos sus caracteres son consecutivos) suma
      `8·n + 5` (`RUN_PER_CHAR`, `RUN_BASE`). Derivado: cualquier otra alineación de un
      término de `n` caracteres da como mucho `21 + 13(n−1) = 13n + 8`; la corrida da
      al menos `9 + 5(n−1) = 5n + 4`, así que el bonus tiene que superar `8n + 4`. Sin
      él "pdf" puntuaba igual (19) "Añadir PDF" que "Campo de texto formulario" y el
      empate iba por orden de catálogo. Rojo → verde: test nuevo
      `a_whole_word_run_beats_any_scattered_alignment` (1 falla / 20 pasan; luego
      21/21). Cambia una expectativa existente:
      `a_contiguous_run_is_not_broken_for_a_word_start` fijaba el valor exacto de "rot"
      en "Rotate options", que ahora suma `run_bonus(3)` porque "rot" es prefijo de
      "Rotate"; lo que el test protege (la corrida no se rompe) no cambia.
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
      *(Añadido posterior, 2026-11-13: ya hay rotar en la paleta, como "rotar todas las
      páginas" sobre el mismo camino que los botones de bloque; ver "Añadido posterior —
      rotar todas las páginas", tras T-234.)*
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
- [x] T-230 `palette/view.rs` + `mod.rs`: overlay, `SearchEntry`, `ListView` con
      etiqueta, atajo y motivo de deshabilitado; navegación por teclado; ejecutar y
      cerrar; devolver el foco. Acción `win.command-palette` con Ctrl+K. [PaletteUI]
      **(2026-11-13 — completo.** Partido en `palette/view.rs` (widgets, CSS, filas,
      teclas), `palette/recent.rs` (recientes, puro), `palette/mod.rs` (`Palette`:
      abrir, cerrar, teclas, ejecutar) y `palette/tests.rs`. **Overlay:** la raíz de
      la ventana pasa a ser un `gtk::Overlay` que envuelve el `content` de siempre;
      un `Overlay` mide y asigna solo por su hijo principal, así que el layout de
      debajo no cambia. Encima va una capa a pantalla completa (`palette-layer`,
      oculta mientras la paleta está cerrada) con la tarjeta centrada arriba
      (`SearchEntry` + lista). Un clic en la capa fuera de la tarjeta la cierra.
      **`ListBox`, no `ListView`:** son ~35 filas que se reconstruyen enteras en cada
      tecla; un `ListView` pide modelo, fábrica y binding por fila para un reciclado
      que 35 filas no necesitan. Cada fila: etiqueta en el idioma activo, el atajo
      (`gtk::accelerator_get_label` sobre la constante de `shortcuts.rs`, "Ctrl+O") y,
      si está rechazada, el motivo debajo, la etiqueta atenuada (`palette-refused`) y
      el estado accesible `Disabled`. Las filas rechazadas **siguen siendo
      seleccionables** (no `set_sensitive(false)`), para que el motivo se lea. Las
      filas no toman foco: el cursor se queda en el campo. **Teclas:** un
      `EventControllerKey` en fase captura sobre la tarjeta: ↑/↓ mueven la selección
      (con tope en los extremos), Esc cierra; Enter es el `activate` del campo; un clic
      en una fila la ejecuta. Una rechazada no hace nada y la paleta queda abierta.
      **Recientes (decisión 8):** `Recent` en memoria dentro de la propia `Palette`
      (vive lo que la ventana, no se guarda en disco); con la consulta vacía van
      primero, el más reciente arriba, y luego el catálogo; una entrada cuenta como
      reciente solo si de verdad corrió. **Foco:** al abrir se guarda un `WeakRef` del
      widget con foco y el cursor va al campo con la consulta vacía. Al cerrar, el
      foco se mueve **antes** de ocultar la capa (ocultar el campo enfocado dejaría
      que GTK elija, y el viewport del canvas persigue esa elección: la trampa de
      `release_focus` en el editor de contenido): vuelve al widget anterior si sigue
      mapeado en esta ventana, y si no, a ninguno (`set_focus(None)`, la misma
      respuesta segura de `release_focus`); además se fijan las dos adjustments del
      canvas alrededor del `grab_focus`, como `focus_without_scrolling`. **Ejecutar:**
      primero se cierra la paleta (foco devuelto) y después corre la entrada, así un
      diálogo, un selector o un cambio de pantalla arrancan desde la ventana tal como
      estaba; si la entrada resulta rechazada en ese instante, el motivo va a la barra
      de estado. **Buscar (decidido):** sin documento sigue rechazada con el texto
      existente `search-open-first`; con documento y otra pantalla visible, la entrada
      lleva antes al editor (`home::show_editor`) y luego activa `win.find`, que ya
      encuentra su popover mapeado. **Ctrl+K:** `shortcuts::COMMAND_PALETTE`
      (`win.command-palette`, `<Control>k`) en la tabla compartida, registrado en
      `connect_standard_shortcuts` (que ahora recibe la `Palette`); vuelve a pulsarse
      y cierra. **Medición de la decisión 6:** GDK no expone cómo construir un evento
      de teclado, así que no se envió un Ctrl+K sintético; el test
      `gtk_ui_palette_ctrl_k_is_resolved_before_a_focused_text_field` enfoca un
      `gtk::Entry` dentro de la ventana y recorre los controladores de la ventana:
      el atajo `win.command-palette` con disparador `<Control>k` aparece en **dos**
      `ShortcutController` de la ventana, **ambos en fase `Capture`**. La captura
      corre desde la ventana hacia abajo, antes que los key bindings del `GtkText`
      enfocado, así que **Ctrl+K llega a la acción aunque un cuadro de texto tenga el
      foco**. No se agregó manejo
      especial. No se verificó con una pulsación real (xdotool) en la app.
      **Esc con una selección de contenido:** Esc es también el acelerador de
      `win.clear-content-selection` (`content_edit::multi`), que corre en la misma
      captura antes que la paleta. `ViewerState::palette_open` lo ponen y lo quitan solo
      `Palette::open`/`close`, y `update_clear_action` deja la acción apagada mientras
      está en `true` (una acción deshabilitada no consume su tecla); `open` y `close`
      la recalculan desde el estado, así que ningún cierre (Esc, Ctrl+K, clic afuera,
      ejecutar una entrada: todos pasan por `close`) la deja apagada. El primer Esc
      cierra la paleta y la selección queda intacta (pedido del coordinador tras la
      primera entrega; test `gtk_ui_palette_escape_closes_it_first_and_keeps_a_content_selection`,
      rojo antes del arreglo: 60 pasan / 1 falla con el filtro `palette`, luego 61/61). **CSS:** `PALETTE_CSS` en
      `view.rs`, sumado a `shell::all_sheets`, solo con colores de la paleta de
      `theme` (`@vitela_*`, `alpha(...)`), así que claro y oscuro salen solos; el test
      `the_sheets_carry_no_hardcoded_colours` rechazó un primer `rgba()`. **Claves
      Fluent nuevas** en `en.ftl` y `es.ftl`: `palette-title`,
      `palette-search-placeholder` ("Search commands…"/"Buscar comandos…") y
      `palette-no-results`. `entries` perdió su `allow(dead_code)`.
      **Correcciones de la revisión de T-229** (hechas primero): (1) `PaletteEntry::run`
      vuelve a consultar `availability` y, si está rechazada, no corre nada y devuelve
      `Err(motivo)`; un único chequeo central en vez de uno por destino, porque
      `set_active(true)` en un toggle insensible igual corre su handler y las entradas
      de firma abrían el selector sin gate. Se corrigió el doc comment que decía que el
      botón volvía a rechazar. (2) `Target::Action` lleva ahora un enum cerrado
      `WindowAction` (`Open`, `New`, `Save`, `Print`, `Undo`, `Redo`, `Find`) y
      `WindowAction::shortcut` es un `match` exhaustivo; desapareció el `unreachable!`.
      También los doc comments de `ClickTool` (Comprimir entra por `open_tool`),
      `invoke.rs` y `entries/mod.rs` (el cambio de pantalla previo y el
      `is_enabled()` de deshacer/rehacer).
      **Evidencia TDD:** parte A: 2 tests nuevos (anotación rechazada por `/P` deja la
      herramienta sin armar; firma sin documento devuelve su motivo y no cambia de
      pantalla), rojos contra `run` sin chequeo (filtro `palette`: 44 pasan / 2
      fallan), verdes tras el chequeo central. Parte B: 8 tests `gtk_ui_palette_*`
      (Ctrl+K abre con el cursor en el campo en Home, editor y Organizar; Esc devuelve
      el foco a un botón sobre el canvas y el canvas no se mueve; widget anterior
      desaparecido → foco en ninguno y canvas quieto; "anotar" + Enter corre Anotar y
      cierra; rechazada muestra el motivo y Enter la deja abierta; recientes primero;
      Buscar desde Home lleva al editor y abre la búsqueda; medición de Ctrl+K) más 4
      de `recent`, 1 de teclas y 1 de texto de atajo. Contra `mod.rs` con cuerpos
      vacíos: 52 pasan / 8 fallan (todos los `gtk_ui_palette_*`; el de medición no
      por la paleta sino porque suponía un solo controlador y hay dos: se corrigió la
      aserción, no el código); tras implementar, 60/60. Gate en
      WSL: `cargo fmt --all -- --check`, `cargo clippy -p linux-gtk --all-targets
      --locked -- -D warnings` y `cargo test -p linux-gtk --locked` en verde: **1017
      pasan, 0 fallan, 1 ignorado**.)
      **Corrección posterior (revisión de T-230).** Los aceleradores `win.*` se
      resuelven en la fase de captura, antes que el campo enfocado. Escribiendo en la
      búsqueda de la paleta (y, ya antes, en el campo Buscar), Ctrl+X cortaba el
      contenido seleccionado de la página, Ctrl+V pegaba el clip o un bitmap en el
      documento, Ctrl+C copiaba la selección del documento y Ctrl+Z/Ctrl+Y deshacían
      o rehacían ediciones del documento. **Arreglo:** `shortcuts.rs` suma `CUT`,
      `COPY` y `PASTE` a la tabla (los tres módulos dejan sus literales) y la lista
      `TEXT_EDITING` (cortar, copiar, pegar, deshacer, rehacer);
      `connect_search_field_handoff`, en un solo lugar y vigilando
      `notify::focus-widget` de la ventana como Supr/F2, **desvincula** esos cinco
      atajos mientras el foco está dentro del campo de la paleta o de
      `viewer.search_entry` (comparados explícitamente, igualdad o `is_ancestor`,
      porque un `GtkEntry` delega el foco en su `GtkText` interno) y los vuelve a
      instalar al salir. **Alcance: solo esos dos campos.** Una primera versión usaba
      `text_input_has_focus` (cualquier campo de texto) y la revisión encontró la
      regresión: un clic en la página no mueve el foco, así que con el foco olvidado
      en, por ejemplo, el `SpinButton` de tamaño de letra de Formularios, Ctrl+Z/C/V
      quedaban muertos mientras el usuario seguía trabajando en la página con el
      mouse. Una consulta nunca es contenido de la página; otros campos sí conviven
      con el trabajo en la página. Por eso los editores de contenido (línea y
      párrafo), los campos de Formularios y el resto **conservan a propósito** el
      comportamiento previo: deshacer y portapapeles del documento. **Por qué
      desvincular y no deshabilitar** (lo que hacen Supr/F2): `win.undo`/`win.redo`
      alimentan también los botones Deshacer/Rehacer de la barra y de Organizar y la
      entrada Deshacer de la paleta, que leen `is_enabled()`; deshabilitarlos
      apagaría esos botones y la paleta diría "nada que deshacer" mientras su propia
      búsqueda tiene el foco. Sin acelerador GTK no tiene nada que despachar y la
      tecla sigue al campo; el estado habilitado nunca se toca, así que al salir no
      hay nada que recalcular (Ctrl+X vuelve a seguir a `update_cut_action`). Un
      `return` temprano en el handler no servía: el acelerador igual consume la
      tecla. **Tests** (`shortcuts::tests::search_field_handoff`, 6 `gtk_ui_*`): GDK
      no permite construir un evento de teclado, así que no se pulsó Ctrl+X real;
      cada test comprueba, como la medición de Ctrl+K, si algún `ShortcutController`
      de la ventana lista la acción, que la acción sigue habilitada según el
      documento y que la selección y la cola de ediciones quedan intactas. Cubren:
      paleta + selección de contenido (Ctrl+X), paleta + clip (Ctrl+V, Ctrl+C), campo
      Buscar (los cinco), cerrar la paleta devuelve Ctrl+X/C/V habilitados y
      Deshacer/Rehacer vinculados (Deshacer apagado sin nada que deshacer),
      Ctrl+Z/Ctrl+Y desvinculados en la paleta con un corte pendiente sin que
      `undo_action` se apague, y un `SpinButton` o un `Entry` cualquiera enfocado
      deja los cinco **vinculados**. **Rojo → verde:** primera versión, contra el
      código previo: 0 pasan / 5 fallan; tras ella, 5/5. Al acotar, el test nuevo
      de "otros campos" contra la versión amplia: 5 pasan / 1 falla; tras acotar,
      6/6. No verificado con una pulsación real (xdotool) en la app. Gate en WSL:
      `cargo fmt --all -- --check`, `cargo clippy -p linux-gtk --all-targets --locked
      -- -D warnings` y `cargo test -p linux-gtk --locked` en verde: **1023 pasan, 0 fallan, 1 ignorado**.
- [x] T-231 Claves Fluent en `en.ftl` y `es.ftl`: etiquetas que falten, sinónimos
      `palette-*-keywords` y el placeholder. Se reutilizan las claves existentes del
      hecho 5 en vez de duplicarlas. [PaletteUI]
      **(2026-11-13 — completo.)** Las 35 entradas tienen sinónimos en los dos
      idiomas: `palette-<id>-keywords`, con los puntos del id cambiados por guiones
      (Fluent no admite puntos). La clave ya no es un campo opcional: la deriva
      `PaletteEntry::keywords_key()` del id, así una entrada no puede apuntar a la
      lista de otra. Cada valor es una lista de palabras separadas por espacios, de
      1 a 10 (test `keyword_lists_stay_short`). **Regla de idioma:** se buscan los
      sinónimos del idioma activo y los ingleses, igual que las etiquetas; los de otro
      idioma no ("fusionar" no encuentra Añadir PDF con la ventana en inglés). La
      etiqueta y sus sinónimos van en un mismo texto por idioma, así una consulta
      puede mezclarlos: "unir pdf" encuentra "Añadir PDF". Ejemplos: "pagina" →
      Organizar primero, luego Extraer y Dividir; "reducir" → Comprimir; "firma" →
      las entradas de firma; "merge" → Añadir PDF en los dos idiomas. **Guarda
      anti-ruido:** "pdf" y "page" en inglés, "pdf" y "pagina" en español devuelven
      como mucho 8 de 35 entradas. Medido: entre 4 y 7, con las legítimas primero;
      el resto son coincidencias dispersas que el matcher ya encontraba sin sinónimos
      ("Export images" contiene p-a-g-e). Ocho deja una entrada de margen y falla en
      cuanto una lista meta esas palabras en comandos ajenos. El placeholder y el
      texto sin resultados ya los había puesto T-230 (`palette-search-placeholder`,
      `palette-no-results`); no faltaba ninguna etiqueta. **Criterio "rotar pagina"
      sigue abierto:** el catálogo no tiene entrada de rotar (ver T-229) y aquí no se
      inventa una. Se cerraría con un comando de núcleo "rotar todas las páginas" que
      la paleta llame igual que un botón. **Rojo → verde:** 7 tests nuevos; contra el
      código sin sinónimos, 5 fallan y 2 pasan (la guarda anti-ruido y la regla de
      idioma, que sin sinónimos se cumplen solas); tras ellos, 7/7. Gate en WSL:
      `cargo fmt --all -- --check`, `cargo clippy -p linux-gtk --all-targets --locked
      -- -D warnings` y `cargo test -p linux-gtk --locked` en verde: **1030 pasan, 0
      fallan, 1 ignorado**.
      *(Añadido posterior, 2026-11-13: la guarda anti-ruido de tope fijo (8) se
      sustituyó por una regla por consulta, por decisión del usuario; ver "Añadido
      posterior — rotar todas las páginas".)*
- [x] T-232 Tabla de atajos compartida (decisión 7): `connect_standard_shortcuts`, la
      paleta y `build_shortcuts_card` leen la misma constante. La tarjeta de Home suma
      Ctrl+K. [PaletteUI]
      **(2026-11-13 — completo.** **Formateador único:** `shortcuts::accel_label(accel)`
      (`gtk::accelerator_parse` + `gtk::accelerator_get_label`), movido desde
      `palette/view.rs` (`shortcut_text`, que desaparece); lo leen las filas de la paleta
      y la tarjeta de Home. Se queda la etiqueta de GTK y no un "Ctrl+" escrito a mano:
      es lo que imprimen los menús y ventanas de atajos de GTK, y GTK traduce los
      nombres de modificador con su propio catálogo, según el locale del sistema (no el
      idioma de Fluent). En C/inglés da "Ctrl+O", el texto de siempre de la tarjeta.
      **No medido** en español: la imagen de WSL no trae el catálogo `gtk40` de `es`,
      y con `LANGUAGE=es` sigue saliendo "Ctrl+O". **Tarjeta:** `home::tools::
      SHORTCUT_CARD`, `[(clave, Shortcut); 6]`: Abrir, Nuevo, Guardar, Buscar, Imprimir
      (los cinco de antes, mismo orden y mismas claves) y la paleta, con la clave
      existente `palette-title` ("Command palette" / "Paleta de comandos"), sin claves
      nuevas en `en.ftl`/`es.ftl`. No las once: cortar, copiar, pegar, deshacer y
      rehacer los enseña cualquier campo de texto, y la tarjeta vive en la columna
      lateral de 268 px debajo de la rejilla de herramientas y las acciones rápidas;
      once filas la empujan fuera de la vista sin enseñar nada nuevo. La paleta llega
      al resto. Layout y CSS sin cambios: misma `card` y mismo `property_row`, una fila
      más. **Rojo → verde:** 5 tests nuevos (3 en `home/tools.rs`, 1 en
      `palette/view.rs`, 1 en `shortcuts.rs` que sustituye al de `shortcut_text`);
      contra la tarjeta con texto literal, 3 fallan (la tarjeta imprime la tabla y GTK
      tiene registrado cada atajo; paleta y tarjeta escriben igual cada atajo; la fila
      de Ctrl+K en inglés y español) y 2 pasan (la lista de la tabla, que es dato, y el
      formateador movido); tras leer la tabla, 5/5. Ningún test de Home afirmaba
      "Ctrl+O". Gate en WSL: `cargo fmt --all -- --check`, `cargo clippy -p linux-gtk --all-targets --locked -- -D warnings` y `cargo test -p linux-gtk --locked` en verde: **1034 pasan, 0 fallan, 1 ignorado**.)

### Fase 3 — Pruebas y docs
- [x] T-233 Tests `gtk_ui_palette_*` sobre `ui_tests::built_ui()`: Ctrl+K abre en las
      tres pantallas; Esc cierra y el foco vuelve; Enter ejecuta un modo; una entrada
      rechazada por `/P` aparece deshabilitada con el motivo y Enter no hace nada; una
      búsqueda en español sin tildes encuentra su comando. Más la medición de la
      decisión 6. [Palette]
      **(2026-11-13 — cerrada sin código nuevo.** Cada caso llegó con la tarea que lo
      necesitaba, con TDD, así que esta tarea queda como el mapa de dónde vive cada uno:
      - Ctrl+K abre en Home, editor y Organize con el foco en el campo:
        `palette/tests.rs` `gtk_ui_palette_ctrl_k_opens_with_the_cursor_in_the_search_field_on_every_screen`.
      - Esc cierra, el foco vuelve y el canvas no se mueve:
        `gtk_ui_palette_escape_hands_the_cursor_back_and_leaves_the_canvas_where_it_was`,
        `gtk_ui_palette_closing_after_the_previous_widget_left_parks_the_cursor_nowhere`,
        `gtk_ui_palette_escape_closes_it_first_and_keeps_a_content_selection`.
      - Enter ejecuta y cierra: `gtk_ui_palette_typing_a_command_and_pressing_enter_runs_it_and_closes`;
        un modo cae donde cae su botón: `entries/tests.rs` `gtk_ui_running_a_mode_entry_lands_where_its_button_does`.
      - Rechazo por `/P` visible y Enter sin efecto:
        `gtk_ui_palette_a_refused_command_shows_why_and_enter_leaves_it_open`,
        `gtk_ui_an_annotation_refusal_reaches_every_annotation_entry`,
        `gtk_ui_running_a_refused_annotation_entry_leaves_the_tool_unarmed`,
        `gtk_ui_running_a_refused_signing_entry_returns_its_refusal`.
      - Español sin tildes: `gtk_ui_spanish_without_accents_finds_its_commands` y los tests
        de sinónimos de T-231.
      - Decisión 6 (Ctrl+K con un campo de texto enfocado):
        `gtk_ui_palette_ctrl_k_is_resolved_before_a_focused_text_field`. **Es una
        prueba estructural** (los controladores de atajos de la ventana corren en fase de
        captura), no una pulsación real; la revisión de T-230 señaló que pasaría aun sin
        el `Entry` enfocado. Lo que prueba es el orden de fases de GTK, no el foco.
      - Atajos de portapapeles y deshacer en la paleta y en Buscar: los cinco tests de
        `shortcuts.rs` de la corrección posterior a T-230.
      Total de la suite al cerrar: **1034 pasan, 0 fallan, 1 ignorado** (gate de T-232).)
- [x] T-234 README: fila de la paleta en la tabla de Linux. [Docs]
      **(2026-11-13 — fila "Command palette (Ctrl+K)" con Linux en 🚧, no ✅**, más un
      párrafo en "Tools & features". La leyenda del README define ✅ como "hecho y
      probado", y en este repo eso ha significado usarlo en la app que corre (Android
      exigió el pase en emulador). La paleta tiene la suite verde, pero nadie la abrió en
      la app: ni su aspecto en claro y oscuro, ni una pulsación real de Ctrl+K. Pasa a ✅
      cuando alguien la use en la app.)

### Añadido posterior — rotar todas las páginas
- [x] Dos entradas en la categoría Páginas: **Girar todas las páginas a la derecha** /
      **a la izquierda** (`pages.rotate-all-right` / `pages.rotate-all-left`, +90 / −90,
      los mismos deltas que los botones de Organize). Cierra el criterio "rotar pagina".
      **Sin comando de núcleo nuevo**, por decisión del usuario (2026-11-13): se usa el
      `Command::RotatePages` existente. [Palette]
      **(2026-11-13.)** **Camino compartido (decisión 2):** `organize::rotate_all_pages(viewer,
      delta)` en `organize/command.rs` es `rotate_block(viewer, 0, len, delta)`, la misma
      función de los dos botones de giro de la vista Documentos: mismo embudo
      `rotation_command`, un único `Command::RotatePages` (un solo paso de deshacer), los
      mismos mensajes ("Rotated 3 pages right.") y la misma invalidación de miniaturas por
      página. No se reescribió `rotate_block`; los botones no cambian. **Permiso:** el gate
      de `rotation_command` pasó a ser una función con nombre, `organize::rotation_refusal`
      (`page_assembly_refusal`, solo el bit de ensamblado: ver la cabecera de
      `organize/command.rs`), y la paleta la pide para la disponibilidad, así el texto de
      rechazo es el del botón. Sin documento: `organize-no-document`. Documento sin
      modelo o vacío: `rotate_block` lo rechaza como run vacío, sin registrar nada. Sin
      cambio de pantalla: corre donde está el usuario y `refresh_preview` restaura la
      pantalla visible tras su reapertura; desde Organize, cada tarjeta vuelve a
      placeholder y pierde su caché (a los dos tamaños), y la reapertura las renderiza de
      nuevo. Etiquetas nuevas `palette-rotate-all-{right,left}` y sinónimos
      `palette-pages-rotate-all-{right,left}-keywords` en `en.ftl` y `es.ftl` (rotate,
      turn, orientation, landscape, portrait / rotar, girar, página, orientación,
      horizontal, vertical). Medido: "rotar pagina" (es) → las dos entradas primero;
      "rotate" → las dos primero en inglés y con la ventana en español.
      **Rojo → verde:** 6 tests nuevos (3 en `palette/entries/tests.rs`, 3 en
      `organize/tests/rotation/all.rs`) más dos filas en el test de "sin documento".
      Contra el esqueleto (entradas y textos ya en el catálogo, función y
      disponibilidad vacías): 7 fallan (los 3 de `all.rs`, el de deshacer, el de
      rechazo, el de "sin documento" y la guarda anti-ruido) y los 2 de búsqueda pasan
      (dependen solo del catálogo y los textos). Tras la implementación,
      los nuevos pasan; la guarda anti-ruido de tope fijo dio rojo: con 37
      entradas, "page" (en) devolvía 9 (> 8), dos de ellas las nuevas. **Guarda
      reescrita (decisión del usuario, 2026-11-13), sin subir el umbral:** por consulta
      se declaran las entradas que de verdad tratan de ella ("page"/"pagina":
      organizar, extraer, dividir y las dos de rotar; "pdf": nuevo PDF en blanco y
      añadir PDF) y se exige (1) que salgan todas, (2) que vayan todas por delante de
      cualquier coincidencia ajena y (3) como mucho 4 ajenas. Un test aparte
      (`the_narrowness_guard_catches_a_polluted_catalog`) pasa por el matcher real un
      catálogo sintético: limpio pasa; con "page" en una entrada ajena falla la regla 2;
      con 5 coincidencias dispersas, la 3. **Medido:** "page" (en) 4 ajenas, "pagina"
      (es) 3, "pdf" (en) 2, todas detrás de las relevantes; "pdf" (es) incumplía la regla 2:
      "Añadir PDF" empataba (19) con "Campo de texto", "Grupo de opciones" y "Lista
      desplegable" y el empate iba por orden de catálogo. **Corregido en el matcher**
      (decisión del usuario, no en el orden del catálogo): bonus de corrida entera
      `8·n + 5` para un término que coincide como palabra o prefijo de palabra (ver la
      corrección posterior en T-228). **Medido tras el arreglo:** ajenas por consulta
      "page" (en) 4, "pagina" (es) 3, "pdf" (en) 2, "pdf" (es) 4, siempre detrás de
      las relevantes. Gate en WSL: `cargo fmt --all -- --check` y `cargo clippy -p
      linux-gtk --all-targets --locked -- -D warnings` en verde; `cargo test -p
      linux-gtk --locked`: **1043 pasan, 0 fallan, 1 ignorado**.

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

**Estado al cerrar (2026-11-13).** Cumplidos y cubiertos por tests: el primero, el
segundo (modos y herramientas de anotación, por el mismo camino que su botón), el tercero,
el cuarto, el quinto y el sexto. El cuarto ("rotar pagina") se cerró después del cierre
del batch: dos entradas, Girar todas las páginas a la derecha / a la izquierda, sobre el
`Command::RotatePages` existente a través de `rotate_block` (el camino de los botones de
giro de bloque), sin comando de núcleo nuevo, por decisión del usuario (2026-11-13); ver
"Añadido posterior — rotar todas las páginas". **Abierto:**
- Revisión visual en la app que corre: aspecto en claro y oscuro y una pulsación real de
  Ctrl+K, también con un campo de texto enfocado (nota de T-234).

## Fuera de scope (v1)

Windows, Android, macOS y web: cada shell tendrá su ficha o su tarea cuando esta cierre
· Buscar dentro del documento o en archivos recientes desde la paleta (Ctrl+F y el
buscador de Home ya lo hacen) · Atajos configurables por el usuario · Guardar los
recientes en disco · Migrar todo el shell a `gio::SimpleAction` (decisión 2).

## Orden de ejecución

Fase 1 → 2 → 3, TDD estricto. T-228 primero y solo, porque es la única pieza que se
puede probar entera sin GTK. T-229 antes que la UI, para que el overlay nazca leyendo un
catálogo real.
