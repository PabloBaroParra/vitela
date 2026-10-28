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

- [ ] T-206 **P1 — Trazados vectoriales como item.** `PathItem {id, page, bbox, …}` en
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
- [ ] T-208 **P3 — Multi-selección y movimiento en bloque.** Selección = conjunto de
      `ContentItemId` (texto, imagen, trazado, grupo). Marquee: items cuyo bbox **toca**
      (interseca) el rectángulo — decidido por el usuario 2026-10-28. SHIFT+click alterna. Arrastre mueve todos
      con un único comando atómico (un paso de undo, igual criterio que `MovePages`).
      Un "objeto tabla" = trazados + texto dentro de su bbox, armado por esta selección.
- [ ] T-209 **P4 — Portapapeles de contenido.** CTRL+X = copiar los spans seleccionados
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
