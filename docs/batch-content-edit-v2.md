# Ficha de batch B25 — Edición de contenido v2 (tablas, multi-selección, portapapeles, recorte)

> Pedido del usuario (2026-10-28), a partir de un presupuesto real generado por Crystal
> Reports ("DOMINGO MIMI.pdf", 3 páginas). Esta ficha es el backlog: lista lo pedido,
> lo que ya existe y lo que falta. Todavía no hay diseño aprobado.

**Dependencias:** B21 ✓ (`core/pdf-edit`: leer/mover/borrar runs, mover/redimensionar/
borrar/reemplazar imágenes, insertar texto/imagen).

## Lo que muestra el PDF de referencia

Se decodificó el content stream de la página 1 (68 `Tj`, 20 `TJ`, 19 `re`, 18 `BT`, 3 `Do`):

- **Las "tablas" no son objetos.** El recuadro del cliente (arriba a la derecha) es un
  `319.05 135.5 251 125 re` + `S`; la tabla de totales son más `re`/`S` sueltos. Para el
  modelo de B21 son invisibles: `PageContent` sólo conoce `text_runs` e `images`. **No hay
  nada que seleccionar** hasta que exista un tipo de item para trazados vectoriales.
- **El texto está partido en fragmentos.** "PRESUPUESTO Nº 3.725/1" son tres operadores
  (`(PRESUPUES) Tj`, `[(T)17(O N° 3)] TJ`, `(.725/1) Tj`). Por eso la app "diferencia por
  palabras": cada `Tj`/`TJ` es un `TextRun`.
- **El pie "Pag 1" es un run más** (`(Pag 1) Tj`), y cada página repite el suyo.
- Hay 3 imágenes por `Do` (`/img1`, logo y firma).

## Pedidos

| # | Pedido | Hoy | Falta |
|---|--------|-----|-------|
| 1 | Mover/redimensionar la tabla de arriba a la derecha (recuadro del cliente + su texto) | ❌ | Trazados vectoriales como item (P1) + grupos (P3) |
| 2 | Mover textos libremente | 🟡 `move_text_run` existe, uno por uno | Arrastre libre en todos los shells; sobre grupos (P2) |
| 3 | Selección múltiple: rectángulo de arrastre (marquee) y SHIFT+click; mover todo junto con el ratón | ❌ | Modelo de selección + comando de movimiento en bloque, un solo undo (P3) |
| 4 | CTRL+X / CTRL+V pegando **exactamente donde está el cursor** | ❌ | Portapapeles de contenido de página (P4) |
| 5 | Texto seguido = un bloque, no palabra por palabra | ❌ | Agrupar runs contiguos en líneas/párrafos (P2) |
| 6 | Recortar imágenes | ❌ | Recorte por clip, sin tocar las muestras (P5) |
| 7 | Quitar textos de pie/encabezado | 🟡 `remove_text_run` existe, uno por uno | "Quitar de todas las páginas" detectando el run repetido (P6) |
| 8 | Editar libremente la tabla de abajo (totales) | 🟡 el texto se puede retipear; las líneas no existen | P1 + P2; editar celdas = retipeo de runs agrupados |

## Fases propuestas

- [x] T-206 **P1 — Trazados vectoriales como item.** Core + FFI + Windows (2026-10-28),
      Linux (2026-10-31), Android (2026-10-31, tests JVM; sin probar en emulador). Como quedó:
      `PathItem {id, page, bbox, paint}` en `PageContent.paths`; el FFI lo lee aparte con
      `read_page_paths` para no cambiar `FfiPageContent` en los shells que todavía no
      editan trazados. Mover y redimensionar reescriben las coordenadas (no se usa `cm`).
      Una segunda edición de geometría antes de guardar se pliega en la que está en cola,
      que conserva el snapshot original, así que mover y después redimensionar es un solo
      undo. `RemovePath` tiene como inverso `RestorePath`, que nunca se escribe (mismo
      criterio que `RemoveWatermark`). El diseño original era: `PathItem {id, page, bbox, …}` en
      `PageContent`: un `re`/`m`/`l`/`c` … hasta el operador de pintado (`S`/`f`/`B`/`n`)
      es un item. Mover = prefijar `cm` (mismo truco que imágenes); borrar = quitar el span.
      Redimensionar una tabla **cambia sólo el marco, no el texto** (decidido por el usuario
      2026-10-28): se reescriben las coordenadas del trazado en vez de escalar con `cm`, así
      el grosor de línea (`w`) no cambia; el texto de adentro no se escala ni se mueve. Ojo con los `re` + `W n` (clip, no se pintan): no son items.
- [ ] T-207 **P2 — Agrupar texto.** Unir runs contiguos (misma línea base, mismo `Tf`,
      separación ≤ ~1 espacio) en un bloque seleccionable como unidad; varias líneas
      alineadas forman párrafo. Es agrupación para **seleccionar/mover/borrar/retipear**,
      no reflow: el "Fuera de scope" de B21 sigue vigente. Retipear un bloque partido en
      varios runs = reescribir el primero y vaciar el resto.
      **Core + FFI + Windows hechos (2026-10-29), Linux y Android (2026-10-31; Android sólo con tests JVM, sin probar en emulador); faltan los párrafos.**
      Windows: Edit content contornea bloques; clic = un editor sobre el bloque entero
      (escribe con `retype_text_block`, cada pausa se pliega en el mismo undo); arrastrar
      más de 4 px mueve el bloque; Delete en el editor borra el bloque. `TextBlockSmoke`.
      Como quedó: `Command::Batch(Vec<Command>)` genérico = un paso de undo (decidido por
      el usuario 2026-10-29; lo reusan T-208 y T-211). Sólo lleva ediciones de contenido de
      un item (`Command::is_batchable`); `pdf-save` lo valida **en secuencia** sobre una
      copia por página, como lo reproduce el guardado. `pdf_edit::group_text_runs` agrupa
      sólo **líneas** (misma fuente, misma altura, hueco ≤ 0.4 × alto); los párrafos
      quedan afuera porque retipear uno pondría todas sus líneas en la primera.
      Retipear = **primero borrar el resto, después reescribir el primero**: al revés, el
      texto más largo corre el cursor y el guardado ya no encuentra los runs siguientes.
      FFI: `read_page_text_blocks`, `move_text_block`, `remove_text_block`,
      `retype_text_block`. Mover un bloque dos veces antes de guardar se pliega en un solo
      undo, igual que retipear un bloque otra vez mientras se escribe, y una edición suelta
      del mismo tipo sobre un run de un batch en cola (p. ej. retipear el run que quedó) se
      pliega en ese hijo. Mezclar tipos (mover un bloque retipeado) se rechaza con
      "save the document before editing it again", salvo donde el core puede tomar un
      **checkpoint en memoria** (`pdf_save::checkpoint`; Windows 2026-10-29, Linux
      2026-10-31): escribe la cola en una base nueva y reintenta ahí, y undo/redo cruzan
      el checkpoint de a un paso. Sesiones con PDFs importados, cifrado, firmas o páginas
      cambiadas siguen con el rechazo.
- [ ] T-208 **P3 — Multi-selección y movimiento en bloque.** Selección = conjunto de
      `ContentItemId` (texto, imagen, trazado, grupo). Marquee: items cuyo bbox **toca**
      (interseca) el rectángulo — decidido por el usuario 2026-10-28. SHIFT+click alterna. Arrastre mueve todos
      con un único comando atómico (un paso de undo, igual criterio que `MovePages`).
      Un "objeto tabla" = trazados + texto dentro de su bbox, armado por esta selección.
      **Core + FFI + Windows hechos (2026-10-29), Linux y Android (2026-10-31; Android sólo con tests JVM, sin probar en emulador).**
      Android (táctil, sin SHIFT): mantener apretado y arrastrar dibuja el marquee; mantener sin
      mover selecciona el item de abajo; con selección, un toque agrega o quita un item de la
      misma página y un toque en vacío (o en otra página) la termina; arrastrar desde adentro del
      recuadro punteado mueve todo; chips "N selected" / Delete / Clear. Tras mover queda
      seleccionado lo movido (se vuelve a resolver por id); tras borrar se termina.
      Como quedó: `pdf_edit::selection` arma un `Command::Batch` de `MoveTextRun`/
      `MoveImage`/`MovePath` (o los `Remove*`) de una sola página; el FFI expone
      `move_content_selection` / `remove_content_selection` con un
      `FfiContentSelection {runs, images, paths}` y pasa por la misma compuerta que los
      bloques (`text_blocks::apply_batch_edit`). Arrastrar la selección otra vez antes de
      guardar se pliega en el batch en cola (ahora también para imágenes y trazados), así
      que sigue siendo un undo. Si un miembro ya tenía una edición suelta en cola (p. ej. la
      imagen se movió sola antes), la selección no se pliega: se valida contra la página con
      todo lo pendiente reproducido y entra como su propio paso.
      Windows: en Edit content, apretar donde no hay texto, imagen ni trazado arranca un
      marquee (rectángulo punteado) que selecciona lo que toca; SHIFT+clic agrega o quita un
      item (y extiende la selección simple que hubiera); SHIFT+arrastre en vacío suma al
      marquee; arrastrar dentro del recuadro punteado mueve todo; Delete borra todo; Esc
      deselecciona. `ContentSelectionSmoke`. Mover a **otra página** queda para T-209
      (cortar/pegar), que es donde se copian recursos.
- [ ] T-209 **P4 — Portapapeles de contenido.**
      **Core + FFI + Windows hechos (2026-10-29), Linux (2026-10-31); falta Android (T-212).**
      Como quedó: `ContentClip {pdf, bbox}` es un PDF de una página (los ítems
      repintados con su CTM, colores, `w`/`J`/`j`/`M`/`d` y fuente, y copia profunda de
      los recursos que nombran). `pdf_edit::copy_selection` / `paste_clip`; pegar importa
      cada recurso como `/VClip{n}` y agrega el contenido al final de la página en su
      `q … Q`. `Command::PasteContent {page, clip, to}` es un paso de undo; su inverso
      `RemovePastedContent` nunca se escribe (como `RestorePath`). El overlay de
      pendientes muestra lo pegado con ids sintéticos `BASE + índice + (k << 24)`.
      FFI: `copy_content_selection`, `cut_content_selection` (copia + borrado de T-208),
      `paste_content(page, clip, left, top)`. Arrastrar **todo** lo recién pegado
      enmienda el `to` del pegado (un solo undo); editar una parte, borrarlo o copiarlo
      antes de guardar se rechaza. Copiar algo con una edición sin guardar se rechaza
      (la copia sale del archivo). Contenido dentro de Form XObjects: todavía no.
      Entre documentos ya funciona con el mismo formato (test FFI y smoke de Windows).
      Windows: Ctrl+C/X/V en Edit content, pegado donde está el puntero.
      `ContentClipboardSmoke`. CTRL+X = copiar los spans seleccionados
      (+ recursos que referencian: fuentes, XObjects) y borrarlos; CTRL+C igual sin borrar;
      CTRL+V reinserta con el ancla (esquina sup-izq del bbox de la selección) en la
      posición del cursor sobre la página. Pegar en otra página exige copiar recursos con
      nombres nuevos (trampa ya vista en insert: reutilizar `/F1` choca).
      **Alcance (decidido por el usuario 2026-10-28): mismo documento primero, entre
      documentos después.** El portapapeles no puede guardar `ObjectId`s del documento de
      origen: tiene que ser un paquete autocontenido (spans + copia profunda de fuentes y
      XObjects), para que pegar en otro PDF importe los recursos igual que `graft_pages`.
      Entre documentos se entrega como segunda etapa sobre ese mismo formato.
- [ ] T-210 **P5 — Recortar imagen.** Envolver el `Do` en `q … re W n … Q` con el rect de
      recorte; las muestras no se tocan (reversible, sin pérdida). Opcional después:
      "aplicar recorte" que resamplea y descarta lo oculto (reusa el camino de compresión).
- [ ] T-211 **P6 — Encabezado/pie.** Detectar runs que se repiten en la misma posición en
      ≥ 2 páginas (texto igual o patrón "Pag N") y ofrecer borrarlos en todas de una, un
      solo undo.
- [ ] T-212 UI por shell (Linux → Windows → Android): marquee, SHIFT, arrastre, atajos,
      manijas de recorte. Android: sin SHIFT/CTRL — selección múltiple por modo, pegar con
      long-press.
