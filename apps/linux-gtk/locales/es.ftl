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
