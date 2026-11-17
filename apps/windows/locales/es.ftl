# Spanish strings for the Windows (WinUI) shell. Must define exactly the same
# message ids as en.ftl (a facade test enforces it). Shared keys keep the GTK
# shell's Spanish (apps/linux-gtk/locales/es.ftl) verbatim.

## Home: header

home-search-placeholder = Buscar archivos recientes y herramientas
home-open-file = Abrir archivo
home-open-file-tooltip = Abrir un PDF (Ctrl+O)

## Home: welcome and drop zone

home-welcome-title = Te damos la bienvenida a Vitela
home-welcome-subtitle = Tu espacio de trabajo PDF, rápido y privado.
home-start-task = Empieza una tarea nueva
home-drop-prompt = Arrastra y suelta un PDF aquí
home-drop-hint = o haz clic en cualquier parte de esta zona para abrir uno
home-select-file = Seleccionar archivo
home-drop-not-local = Aquí solo se pueden soltar archivos locales.
home-drop-not-pdf = Desde la pantalla de inicio solo se pueden abrir archivos PDF.

## Home: recent documents

home-recent-title = Recientes
home-recent-today = Hoy
home-recent-yesterday = Ayer
home-recent-earlier = Anteriores
home-recent-empty = Aún no hay documentos recientes. Abre un PDF y aparecerá aquí.
home-recent-no-match = Ningún documento reciente coincide con la búsqueda.
home-recent-open-card = Abrir { $name }
home-recent-remove = Quitar de Recientes
home-recent-remove-failed = No se pudo quitar { $name } de Recientes.
home-recent-opened-today = Abierto hoy, { $time }
home-recent-opened-yesterday = Abierto ayer, { $time }
home-recent-opened-on = Abierto el { $date }
home-recent-page-count =
    { $count ->
        [one] { $count } página
       *[other] { $count } páginas
    }

## Home: tool grid

home-tools-title = Herramientas
home-tool-unavailable = Aún no disponible
home-tool-edit = Editar
home-tool-edit-description = Reescribe texto y reemplaza imágenes
home-tool-annotate = Anotar
home-tool-annotate-description = Resalta, dibuja y añade notas
home-tool-sign = Firmar
home-tool-sign-description = Firma con un certificado, una tarjeta o un token
home-tool-organize = Organizar
home-tool-organize-description = Reordena y elimina páginas
home-tool-compress = Comprimir
home-tool-compress-description = Guarda una copia más pequeña del archivo
home-tool-protect = Proteger
home-tool-protect-description = Exige una contraseña para abrir el documento

## Home: quick actions

home-quick-actions-title = Acciones rápidas
home-quick-new-blank = Nuevo PDF en blanco
home-quick-open = Abrir archivo…
home-quick-sample = Abrir el ejemplo

## Home: keyboard shortcuts

home-shortcuts-title = Atajos de teclado
home-shortcut-open = Abrir
home-shortcut-new = Nuevo
home-shortcut-save = Guardar
home-shortcut-find = Buscar
home-shortcut-print = Imprimir
home-shortcut-command-palette = Paleta de comandos

## Home: version footer

home-version = Vitela { $version }
home-version-dev = desarrollo

## Home: Windows only

home-return-to-document = Volver al documento
home-recent-files = Archivos recientes

## Mode bar (top of the window)

mode-bar-sections = Secciones de Vitela
mode-home = Inicio
mode-edit = Editar
mode-annotate = Anotar
mode-organize = Organizar
mode-sign = Firmar
mode-protect = Proteger

## Editor: toolbar groups (accessible names)

editor-group-document = Documento
editor-group-output = Salida
editor-group-history = Historial
editor-group-position = Posición
editor-group-zoom = Zoom
editor-group-fit = Ajuste
editor-group-panels = Paneles
editor-group-find = Buscar

## Editor: document commands, navigation and search

editor-open = Abrir PDF
editor-open-short = Abrir
editor-open-tooltip = Abrir PDF (Ctrl+O)
editor-sample = Abrir ejemplo
editor-sample-plain = Ejemplo de Vitela
# Keep the sample passwords unchanged: users enter them to open the files.
editor-sample-aes128 = Ejemplo AES-128 (user-aes-pass)
editor-sample-rc4128 = Ejemplo RC4-128 (user-rc4-pass)
editor-save = Guardar como
editor-save-tooltip = Guardar como (Ctrl+S)
editor-print = Imprimir
editor-print-tooltip = Imprimir (Ctrl+P)
editor-export = Exportar imágenes
editor-export-tooltip = Exportar páginas como imágenes
editor-undo = Deshacer
editor-redo = Rehacer
editor-page-position-tooltip = Página actual / total de páginas
editor-zoom-out = Alejar
editor-zoom-in = Acercar
editor-fit-width = Ajustar al ancho
editor-fit-page = Ajustar a la página
editor-pages = Páginas
editor-tools = Herramientas
editor-find = Buscar en el documento
editor-find-tooltip = Buscar en el documento (Ctrl+F)
editor-search-document = Buscar en el documento
editor-find-previous = Coincidencia anterior
editor-find-next = Coincidencia siguiente
editor-find-hint = Distingue mayúsculas y minúsculas. Pulsa Intro para buscar.
editor-idle-status = Elige un archivo PDF para verlo o abre el ejemplo integrado.
editor-tools-panel = Herramientas y propiedades

## Mode bar: Windows only (the rail also lists Recent and My files)

mode-recent = Recientes
mode-files = Mis archivos
mode-edit-pdf = Editar PDF
mode-organize-pages = Organizar páginas

## Editor: Windows only toolbar commands, groups and page navigation

editor-share = Compartir
editor-share-tooltip = Enviar una copia con Uso compartido en proximidad, Vínculo móvil u otra aplicación
editor-undo-tooltip = Deshacer (Ctrl+Z)
editor-redo-tooltip = Rehacer (Ctrl+Y)
editor-pages-tooltip = Mostrar u ocultar la lista de páginas
editor-tools-tooltip = Mostrar u ocultar las herramientas
editor-group-page-navigation = Navegación por páginas
editor-group-more-tools = Más herramientas del documento
editor-group-document-title = Título del documento
editor-first-page = Primera página
editor-first-page-tooltip = Ir a la primera página del documento (Ctrl+Home)
editor-previous-page = Página anterior
editor-previous-page-tooltip = Ir a la página anterior a la visible (Ctrl+Up)
editor-next-page = Página siguiente
editor-next-page-tooltip = Ir a la página siguiente a la visible (Ctrl+Down)
editor-last-page = Última página
editor-last-page-tooltip = Ir a la última página del documento (Ctrl+End)
editor-go-to-page = Ir a la página
editor-go-to-page-tooltip = Saltar a una página por su número (Ctrl+G)
editor-protect = Proteger
editor-compress = Comprimir
editor-extract-pages = Extraer páginas
editor-split-pages = Dividir PDF
editor-split-pages-tooltip = Dividir el documento en varios PDF
editor-organize = Organizar
editor-organize-tooltip = Mover, girar o eliminar páginas
editor-find-submit = Buscar
editor-no-document = No hay ningún documento abierto
editor-empty-state = Suelta aquí un PDF para abrirlo o prueba el ejemplo integrado.

## Pages sidebar and side-column dividers: Windows only

editor-page-item = Página { $page }
editor-resize-pages = Cambiar el tamaño de la lista de páginas
editor-resize-pages-tooltip = Arrastra para cambiar el tamaño de la lista de páginas o plegarla, o usa las flechas izquierda y derecha cuando tenga el foco
editor-resize-tools = Cambiar el tamaño del panel de herramientas
editor-resize-tools-tooltip = Arrastra para cambiar el tamaño del panel de herramientas o plegarlo, o usa las flechas izquierda y derecha cuando tenga el foco
editor-go-to-page-number = Número de página (1–{ $total })
editor-go-to-page-go = Ir
editor-go-to-page-cancel = Cancelar

## Protect: Windows only

# Status line when the rail's Protect is chosen on a document that forbids it.
protect-forbidden = Este documento no permite cambiar su protección.

## Organize: shared with the editor toolbar's Extract pages

organize-extract-tooltip = Guardar las páginas elegidas como un PDF nuevo

## Search: Windows only

search-result = Página { $page }: { $text }
search-navigate-failed = No se pudo ir a la coincidencia: { $error }

## Search: status messages

search-enter-text = Introduce el texto que quieras buscar.
search-open-first = Abre un PDF antes de buscar.
search-running = Buscando «{ $query }»...
search-no-matches = No hay coincidencias para «{ $query }».
search-failed = No se pudo buscar: { $error }
search-match = Coincidencia { $index } de { $total } para «{ $query }».

## Tools panel: page headings

tools-comments = Comentarios
tools-annotations = Anotaciones
tools-comments-unavailable = Los comentarios aún no están disponibles en esta versión.

## Document properties: labels and edit results

metadata-heading = Propiedades del documento
metadata-pages = Páginas
metadata-title = Título
metadata-author = Autor
metadata-subject = Asunto
metadata-keywords = Palabras clave
metadata-creator = Creador
metadata-producer = Productor
metadata-created = Creado
metadata-modified = Modificado
# Display hint only: the accepted date/time format stays YYYY-MM-DD HH:MM:SS.
metadata-date-placeholder = AAAA-MM-DD HH:MM:SS
metadata-open-first = Abre un PDF antes de editar sus propiedades.
metadata-unchanged = No hay cambios que registrar.
metadata-updated = Propiedades del documento actualizadas. Los cambios están pendientes de guardar.
metadata-invalid-date = Fecha no válida; se ha restaurado el valor anterior: { $error }

## Tools panel: Windows only tabs (the GTK shell had them until its panel was
## redesigned; same keys and Spanish as its #542)

tools-pages = Páginas de herramientas
tools-tab-edit = Editar
tools-tab-annotate = Anotar
tools-tab-comments = Comentarios
tools-tab-fill-sign = Rellenar y firmar

## Document properties: Windows only

metadata-pages-value = Páginas: { $count }
metadata-pages-none = Páginas: —
metadata-forbidden = Este documento no permite cambiar sus metadatos.

## Annotate: tool buttons and controls

annotate-tool-highlight = Resaltado
annotate-tool-underline = Subrayado
annotate-tool-strikeout = Tachado
annotate-tool-ink = Tinta
annotate-tool-note = Nota
annotate-tool-text-box = Cuadro de texto
annotate-tool-shape = Forma
annotate-tool-stamp = Sello
annotate-previous = Anotación anterior
annotate-nudge = Desplazar
annotate-grow = Agrandar
annotate-restyle = Cambiar estilo
annotate-delete = Eliminar
annotate-color-title = Elegir el color de la anotación

## Annotate: status messages and refusals

annotate-stamp-added = Sello de imagen añadido. Los cambios están pendientes de guardar.
annotate-undone-pending = Cambio deshecho. Los cambios están pendientes de guardar.
annotate-redone-pending = Cambio rehecho. Los cambios están pendientes de guardar.
annotate-edited = Anotación editada. Los cambios están pendientes de guardar.
annotate-moved = Anotación movida. Los cambios están pendientes de guardar.
annotate-resized = Anotación redimensionada. Los cambios están pendientes de guardar.
annotate-restyled = Estilo de la anotación cambiado. Los cambios están pendientes de guardar.

## Annotate: note and text box dialogs

annotate-note-canceled = Colocación de la nota cancelada.
# $page (Number) - one-based page number.
annotate-note-title = Añadir nota — página { $page }
annotate-note-text = Texto de la nota
annotate-dialog-cancel = Cancelar
annotate-dialog-add = Añadir
annotate-dialog-save = Guardar
annotate-dialog-text = Texto
annotate-text-box-add-canceled = Colocación del cuadro de texto cancelada.
annotate-text-box-edit-canceled = Edición del cuadro de texto cancelada.
# $page (Number) - one-based page number.
annotate-text-box-add-title = Añadir cuadro de texto — página { $page }
annotate-text-box-added = Cuadro de texto añadido. Los cambios están pendientes de guardar.
annotate-text-box-edited = Cuadro de texto editado. Los cambios están pendientes de guardar.

## Annotate: Windows only (toolbar commands, numeric geometry dialogs, note reading)

annotate-pointer = Puntero
annotate-text-box-tooltip = Haz clic o arrastra en una página para añadir un cuadro de texto
annotate-previous-tooltip = Selecciona la anotación anterior, incluidas las que quedan debajo de otras
annotate-next = Anotación siguiente
annotate-next-tooltip = Selecciona la anotación siguiente, incluidas las que quedan debajo de otras
annotate-edit-text-box = Editar cuadro de texto
annotate-edit-text-box-tooltip = Reescribe el cuadro de texto seleccionado
annotate-read-note = Leer nota
annotate-read-note-tooltip = Lee la nota seleccionada sin cambiar el documento
annotate-nudge-tooltip = Desplaza la anotación seleccionada
annotate-move = Mover anotación
annotate-move-tooltip = Establece la X y la Y de la anotación seleccionada en puntos PDF
annotate-grow-tooltip = Agranda la anotación seleccionada
annotate-resize = Cambiar tamaño de la anotación
annotate-resize-tooltip = Establece el ancho y el alto de la anotación seleccionada en puntos PDF
annotate-pointer-mode = Modo puntero.
annotate-text-box-armed = Herramienta Cuadro de texto activada. Haz clic o arrastra en una página para colocarlo.
annotate-armed = Herramienta { $tool } activada. Arrastra en una página para colocar la anotación.
annotate-ink-needs-drag-not-tap = La tinta se dibuja arrastrando, no con un toque.
annotate-image-unsupported = La imagen no es un PNG o JPEG compatible.
annotate-stamp-canceled = Colocación del sello cancelada.
annotate-stamp-unreadable = No se pudo leer la imagen de sello seleccionada.
annotate-selected-on-page = Anotación { $id } de la página { $page } seleccionada.
annotate-move-title = Mover anotación — página { $page }
annotate-move-explanation = Coloca la esquina inferior izquierda de la anotación en el espacio PDF sin rotar. X aumenta hacia la derecha; Y aumenta hacia arriba. El tamaño no cambia.
annotate-position-unchanged = La posición de la anotación no ha cambiado.
annotate-resize-title = Cambiar tamaño de la anotación — página { $page }
annotate-resize-explanation = La anotación mantiene su origen actual en el espacio PDF. El ancho y el alto son independientes; los sellos se estiran para ajustarse.
annotate-dimensions-unchanged = Las dimensiones de la anotación no han cambiado.
annotate-note-read-title = Nota — página { $page }
annotate-dialog-close = Cerrar
annotate-text-box-edit-page-title = Editar cuadro de texto — página { $page }

## Color dialog: Windows only (shared by annotations and form fields)

editor-color-choose = Elegir
editor-color-cancel = Cancelar

## Numeric geometry dialogs: Windows only (annotations, images and text)

geometry-x = X (pt)
geometry-y = Y (pt)
geometry-width = Ancho (pt)
geometry-height = Alto (pt)
geometry-move = Mover
geometry-resize = Cambiar tamaño
geometry-cancel = Cancelar
