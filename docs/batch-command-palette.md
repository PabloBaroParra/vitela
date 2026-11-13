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
      puntaje es la suma. Puntos por carácter: 1, +8 inicio de palabra (comienzo o
      tras espacio, `-`, `_`, `/`), +4 contiguo, +12 si el término arranca en el primer
      carácter. Cada término prueba todas las posiciones de su primer carácter y
      completa el resto de forma voraz: no es una búsqueda exhaustiva de alineaciones,
      pero encuentra la de inicio de palabra. La tabla de tildes es explícita, sin
      crate: minúsculas de Latin-1 Supplement y Latin Extended-A (español, portugués,
      francés, alemán), más `ß`→"ss", `æ`→"ae", `œ`→"oe"; el resto (griego, cirílico,
      CJK, marcas combinantes) pasa solo en minúsculas. El `dead_code` se silencia con
      un único `#![cfg_attr(not(test), allow(dead_code))]` en `matcher.rs` hasta que
      T-229 lo use.
      **Evidencia TDD:** 16 tests escritos primero contra cuerpos `todo!()`: 0 pasan /
      16 fallan; tras implementar, 16/16. Gate en WSL: `cargo fmt --all -- --check`,
      `cargo clippy -p linux-gtk --all-targets --locked -- -D warnings` y
      `cargo test -p linux-gtk --locked palette::` en verde; la suite completa de
      `linux-gtk`: **974 pasan, 0 fallan, 1 ignorado**.)
- [ ] T-229 `palette/entries.rs`: `PaletteEntry { id, label_key, keywords_key,
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
