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

## Signing: certificate section, choosers and unlock prompts
# Adapter and module error details arrive untranslated as { $error }.

sign-heading = Firma digital
sign-choose-pfx = Elegir certificado de firma (.pfx)…
sign-use-token = Usar tarjeta o token…
sign-use-nss = Usar un certificado de este equipo…
sign-signed-indicator = ✓ Este documento está firmado digitalmente.
sign-open = Abrir
sign-cancel = Cancelar
sign-unlock = Desbloquear
sign-not-local-file = La ubicación seleccionada no es un archivo local.
sign-not-local-folder = La ubicación seleccionada no es una carpeta local.
sign-pfx-filter = Certificados PKCS#12
sign-pfx-chooser-title = Elegir certificado de firma
sign-pfx-password-title = Se necesita la contraseña del certificado
sign-pfx-reading = Leyendo el archivo de certificado...
sign-pfx-no-identities = Este archivo de certificado no contiene identidades de firma utilizables.
sign-pfx-waiting = Esperando la contraseña del certificado.
sign-pfx-wrong-password = La contraseña no es correcta o el archivo no es un certificado PKCS#12 válido.
sign-pfx-read-failed = No se pudo leer el archivo de certificado: { $error }
sign-certificate-cancelled = Selección de certificado cancelada.
sign-found-identities =
    { $count ->
        [one] Se encontró { $count } identidad de firma: { $names }
       *[other] Se encontraron { $count } identidades de firma: { $names }
    }
sign-pkcs11-filter = Módulos PKCS#11
sign-pkcs11-chooser-title = Elegir módulo PKCS#11
sign-pkcs11-pin-title = Se necesita el PIN de la tarjeta o el token
sign-pkcs11-reading = Leyendo la tarjeta o el token...
sign-pkcs11-waiting = Esperando el PIN del token.
sign-pkcs11-no-identities = No se encontraron identidades de firma. Comprueba el PIN y que la tarjeta o el token contenga un certificado.
sign-pkcs11-load-failed = No se pudo cargar el módulo PKCS#11: { $error }
sign-pkcs11-cancelled = Selección de tarjeta o token cancelada.
sign-nss-filter = Módulos NSS
sign-nss-module-chooser-title = Elegir el módulo de certificados NSS (libsoftokn3.so)
sign-nss-profile-chooser-title = Elegir la carpeta de la base de datos de certificados
sign-nss-password-title = Contraseña de la base de datos de certificados
sign-nss-password-hint = Déjalo en blanco si tus certificados no tienen contraseña.
sign-nss-reading = Leyendo la base de datos de certificados...
sign-nss-waiting = Esperando la contraseña de la base de datos de certificados.
sign-nss-no-identities = No se encontraron certificados de firma. Comprueba la contraseña y que este equipo tenga certificados importados.
sign-nss-load-failed = No se pudo cargar la base de datos de certificados: { $error }
sign-nss-invalid-path = No se puede usar la ruta de la base de datos de certificados { $path }.

## Signing: identity picker and signing refusals

sign-picker-title = Elegir una identidad de firma
sign-picker-prompt = Firmar este documento con:
sign-picker-sign = Firmar
sign-picker-none-chosen = Elige una identidad de firma.
sign-cancelled = Firma cancelada.
sign-no-document = Abre un PDF antes de firmar.
sign-unsaved-changes = Guarda los cambios antes de firmar este documento.
sign-not-signable = Este documento no se puede firmar.

## Drawn signature: entry, pad, saved-signature offer and placement

signature-draw = Dibujar firma…
signature-draw-tooltip = Dibuja tu firma con el ratón y haz clic en una página para colocarla como imagen
signature-pad-title = Dibuja tu firma
signature-remember = Recordar en este equipo
signature-clear = Borrar
signature-cancel = Cancelar
signature-use = Usar
signature-offer-title = Tu firma
signature-offer-preview = Tu firma guardada
signature-offer-unshowable = Esta firma guardada ya no se puede mostrar.
signature-draw-new = Dibujar otra
signature-delete = Eliminar
signature-no-document = Abre un PDF antes de dibujar una firma.
signature-unrenderable = No se pudo convertir la firma en una imagen.
signature-not-remembered = No se pudo recordar tu firma. Haz clic en una página para colocarla.
signature-forgotten = Tu firma guardada se ha eliminado de este equipo.
signature-armed = Haz clic en una página para colocar tu firma.
signature-disarmed = Firma desactivada.
signature-stale = Esa firma se dibujó para otro documento. Vuelve a dibujarla para colocarla.

## Organize: header, view switch and hints

organize-heading = Organizar páginas
organize-import-progress-name = Progreso de la importación de PDF
organize-cancel-import = Cancelar
organize-undo = Deshacer
organize-redo = Rehacer
organize-add-pdfs = Añadir PDF
organize-extract = Extraer
organize-extract-tooltip = Guardar las páginas elegidas como un PDF nuevo
organize-split = Dividir
organize-split-tooltip = Dividir este PDF en varios PDF nuevos
organize-save = Guardar
organize-view-documents = Documentos
organize-view-pages = Páginas
organize-documents-hint = Arrastra un documento para mover todas sus páginas.
organize-pages-hint = Arrastra una página para cambiarla de posición.

## Organize: page and document cards (accessible names and tooltips)

organize-block-title-part = { $name } — Parte { $part }
organize-block-meta-one = 1 página · { $first }
organize-block-meta = { $count } páginas · { $first }–{ $last }
organize-block-name = { $title }, { $meta }, documento { $position } de { $total }
organize-block-move-up = Subir: { $name }
organize-block-move-down = Bajar: { $name }
organize-block-rotate-left = Girar a la izquierda: { $name }
organize-block-rotate-right = Girar a la derecha: { $name }
organize-block-delete = Eliminar { $name }
organize-blank-pages = Páginas en blanco
organize-blank-page = Página en blanco
organize-imported-pdf = PDF importado
organize-page-rotate-left = Girar la página a la izquierda
organize-page-rotate-right = Girar la página a la derecha
organize-page-delete = Eliminar la página

## Organize: page operation results and refusals

organize-no-document = Abre un PDF antes de organizar sus páginas.
organize-invalid-position = La posición de la página no es válida.
organize-page-gone = La página ya no existe.
organize-pages-gone = Esas páginas ya no existen.
organize-move-page-failed = No se pudo mover la página.
organize-move-document-failed = No se pudo mover el documento.
organize-delete-page-failed = No se pudo eliminar la página.
organize-delete-document-failed = No se pudo eliminar el documento.
organize-rotate-page-failed = No se pudo girar la página.
organize-rotate-document-failed = No se pudo girar el documento.
organize-moved-page = Página { $from } movida a la posición { $to }.
organize-moved-pages =
    { $count ->
        [one] { $count } página movida a la posición { $to }.
       *[other] { $count } páginas movidas a la posición { $to }.
    }
organize-deleted-page = Página { $page } eliminada.
organize-deleted-pages =
    { $count ->
        [one] { $count } página eliminada.
       *[other] { $count } páginas eliminadas.
    }
organize-rotated-page-left = Página { $page } girada a la izquierda.
organize-rotated-page-right = Página { $page } girada a la derecha.
organize-rotated-pages-left =
    { $count ->
        [one] { $count } página girada a la izquierda.
       *[other] { $count } páginas giradas a la izquierda.
    }
organize-rotated-pages-right =
    { $count ->
        [one] { $count } página girada a la derecha.
       *[other] { $count } páginas giradas a la derecha.
    }

## Organize: adding PDFs (chooser, progress, passwords and results)

organize-import-filter = Archivos PDF
organize-import-chooser-title = Añadir PDF
organize-import-accept = Añadir
organize-import-cancelled = Importación de PDF cancelada.
organize-import-changed = Importación de PDF cancelada porque el documento ha cambiado.
organize-import-not-local = Todos los PDF seleccionados deben ser archivos locales.
organize-import-checking = Comprobando los PDF seleccionados...
organize-import-progress = { $completed } de { $total } PDF
organize-import-worker-stopped = El proceso de importación de PDF se detuvo de forma inesperada.
organize-import-unnamed = PDF seleccionado
organize-import-failed = No se pudo importar { $name }: { $error }
organize-import-no-copying = No se pudo importar { $name }: el PDF no permite copiar sus páginas.
organize-import-no-pages = No se pudo importar { $name }: el PDF no tiene páginas.
organize-import-too-many-pages = Los PDF seleccionados tienen demasiadas páginas.
organize-import-source-ids-exhausted = Se han agotado los identificadores de PDF importados.
organize-import-page-ids-exhausted = Se han agotado los identificadores de página.
organize-import-password-title = Se necesita una contraseña
organize-import-password-prompt = Introduce la contraseña de { $name }.
organize-import-password-wrong = La contraseña no es correcta. Inténtalo de nuevo.
organize-import-password-cancel = Cancelar la importación
organize-import-unlock = Desbloquear
organize-import-password-waiting = Esperando la contraseña de { $name }.
organize-import-warning-title = Parte de la información del documento no se conservará
organize-import-warning-cancel = Cancelar
organize-import-anyway = Importar de todos modos
organize-import-apply-failed = No se pudieron añadir los PDF seleccionados.
organize-imported-pages =
    { $count ->
        [one] { $count } página importada.
       *[other] { $count } páginas importadas.
    }
