# Spanish strings for the Linux GTK shell. Must define exactly the same
# message ids as en.ftl (a unit test enforces it).

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

## Home: version footer

home-version = Vitela { $version }
home-version-dev = desarrollo

## Persistent navigation rail

rail-sections = Secciones de Vitela
rail-home = Inicio
rail-recent = Recientes
rail-files = Mis archivos
rail-edit = Editar PDF
rail-annotate = Anotar
rail-sign = Firmar
rail-organize = Organizar páginas
rail-protect = Proteger

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

## Search: status messages

search-enter-text = Introduce el texto que quieras buscar.
search-open-first = Abre un PDF antes de buscar.
search-running = Buscando «{ $query }»...
search-no-matches = No hay coincidencias para «{ $query }».
search-failed = No se pudo buscar: { $error }
search-match = Coincidencia { $index } de { $total } para «{ $query }».

## Tools panel: navigation (page identifiers stay language-independent)

tools-tab-edit = Editar
tools-tab-annotate = Anotar
tools-tab-comments = Comentarios
tools-tab-fill-sign = Rellenar y firmar
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

## Document properties: date validation (field names are localized arguments)

metadata-year = año
metadata-month = mes
metadata-day = día
metadata-hour = hora
metadata-minute = minuto
metadata-second = segundo
metadata-date-missing = se esperaba AAAA-MM-DD; falta el campo «{ $field }»
metadata-date-not-number = se esperaba AAAA-MM-DD; el campo «{ $field }» no es un número
metadata-date-extra = se esperaba AAAA-MM-DD; hay partes adicionales separadas por «-»
metadata-year-overflow = el año { $year } no cabe en el rango admitido
metadata-time-missing = se esperaba HH:MM o HH:MM:SS; falta el campo «{ $field }»
metadata-time-not-number = se esperaba HH:MM o HH:MM:SS; el campo «{ $field }» no es un número
metadata-time-extra = se esperaba HH:MM o HH:MM:SS; hay partes adicionales separadas por «:»
metadata-component-range = el campo «{ $field }» tiene el valor { $value }, fuera del rango ({ $min }-{ $max })

## Edit PDF: panel controls, tooltips and gesture hints

edit-heading = Editar PDF
edit-description = Modifica el texto, las imágenes y las formas que ya están en la página.
edit-card-text = Texto
edit-card-images = Imágenes
edit-card-shapes = Formas
edit-card-pages = Páginas
edit-mode = Editar contenido
edit-mode-tooltip = Activa la edición de contenido y haz clic en el texto de la página para seleccionarlo
edit-insert-text = Insertar texto
edit-insert-text-tooltip = Haz clic en la página para colocar un nuevo cuadro de texto
edit-delete-text = Eliminar texto
edit-delete-text-tooltip = Elimina de la página el texto seleccionado o el que estás reescribiendo
edit-insert-image = Insertar imagen
edit-insert-image-tooltip = Haz clic en la página para insertar una imagen
edit-replace-image = Reemplazar imagen
edit-replace-image-tooltip = Sustituye la imagen seleccionada por un archivo del disco
edit-delete-image = Eliminar imagen
edit-delete-image-tooltip = Elimina de la página la imagen seleccionada
edit-delete-shape = Eliminar forma
edit-delete-shape-tooltip = Elimina de la página la línea, el borde o el recuadro seleccionado
edit-watermark = Añadir marca de agua
edit-watermark-tooltip = Aplica el mismo texto en las páginas que elijas
edit-watermark-hint = La marca de agua se incorpora al contenido de las páginas y se deshace en un solo paso.
edit-no-document = Abre un PDF para editar el texto y las imágenes de sus páginas.
edit-no-text-selected = Haz clic en el texto para seleccionarlo; haz doble clic para reescribirlo o arrástralo para moverlo.
edit-text-selected = Texto seleccionado: arrástralo para moverlo, pulsa Supr para eliminarlo o haz doble clic o pulsa F2 para reescribirlo.
edit-text-editing = Reescribiendo texto: pulsa Intro para conservarlo o Esc para cancelar, o elimínalo.
edit-no-image-selected = Haz clic en una imagen de la página para seleccionarla.
edit-image-selected = Imagen seleccionada: puedes reemplazarla o eliminarla.
edit-image-no-replace = Imagen seleccionada: puedes moverla, cambiar su tamaño o eliminarla. No se puede reemplazar porque no es posible recuperar su codificación actual para deshacer el cambio.
edit-image-pending = Esta imagen tiene un cambio sin guardar: puedes moverla, cambiar su tamaño o eliminarla, o guardar antes de reemplazarla.
edit-no-shape-selected = Haz clic en una línea, un borde o un recuadro sombreado de la página para seleccionarlo.
edit-shape-selected = Forma seleccionada: arrástrala para moverla, arrastra una esquina para cambiar su tamaño o pulsa Supr.

## Edit PDF: mode activation messages

edit-mode-armed = Edición de contenido activada: haz clic en el texto para seleccionarlo o haz doble clic para reescribirlo.
edit-mode-disarmed = Edición de contenido desactivada.
edit-insert-text-armed = Inserción de texto activada: haz clic en la página para colocar un nuevo cuadro de texto.
edit-insert-image-armed = Inserción de imágenes activada: haz clic en la página para insertar una imagen.
