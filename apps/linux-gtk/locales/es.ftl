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
edit-quick-actions = Acciones rápidas
edit-quick-retype = Editar texto
edit-quick-retype-tooltip = Reescribe el texto seleccionado en su sitio (F2)
edit-quick-replace = Reemplazar
edit-quick-cut = Cortar
edit-quick-cut-tooltip = Corta la selección al portapapeles (Ctrl+X)
edit-quick-copy = Copiar
edit-quick-copy-tooltip = Copia la selección al portapapeles (Ctrl+C)
edit-quick-delete = Eliminar
edit-quick-delete-tooltip = Elimina la selección de la página (Supr)
edit-watermark = Añadir marca de agua
edit-watermark-tooltip = Aplica el mismo texto en las páginas que elijas
edit-watermark-hint = La marca de agua se incorpora al contenido de las páginas y se deshace en un solo paso.
edit-no-document = Abre un PDF para editar el texto y las imágenes de sus páginas.
edit-no-text-selected = Haz clic en el texto para seleccionarlo; haz doble clic para reescribirlo o arrástralo para moverlo.
edit-text-selected = Texto seleccionado: arrástralo para moverlo, pulsa Supr para eliminarlo o haz doble clic o pulsa F2 para reescribirlo.
edit-text-editing = Reescribiendo texto: pulsa Intro para conservarlo o Esc para cancelar, o elimínalo.
edit-paragraph-editing = Reescribiendo un párrafo: Intro empieza una línea nueva, Ctrl+Intro o hacer clic fuera lo conserva, Esc cancela.
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

## Signing: identity picker, refusals and writing the signed PDF

sign-picker-title = Elegir una identidad de firma
sign-picker-prompt = Firmar este documento con:
sign-picker-sign = Firmar
sign-picker-none-chosen = Elige una identidad de firma.
sign-cancelled = Firma cancelada.
sign-chooser-title = Guardar el PDF firmado
sign-busy = Firmando el PDF...
sign-done = PDF firmado y vuelto a abrir.
sign-failed = No se pudo firmar el PDF: { $error }
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

## Write chains: shared by every write (destination chooser, replace guard,
## signature warning). File-system details arrive untranslated as { $error }.

write-no-save-backing = Este documento no tiene una copia base desde la que guardarse.
write-pdf-filter = Archivos PDF
write-accept = Guardar
write-not-local-file = La ubicación seleccionada no es un archivo local.
write-check-failed = No se pudo comprobar si { $path } ya existe: { $error }
write-replace-title = ¿Reemplazar el PDF existente?
write-replace-cancel = Cancelar
write-replace = Reemplazar
write-signature-title = Al guardar se romperá la firma de este documento
write-signature-detail =
    Este documento está firmado. Al guardar se reescribe el archivo, así que la firma dejará de coincidir con lo que cubre.

    No se elimina: el archivo guardado sigue llevando la firma, y los lectores de PDF la mostrarán como no válida, no como ausente.

    Para conservar una copia que siga siendo válida, cancela y guarda en otro archivo.
write-signature-cancel = Cancelar
write-signature-proceed = Guardar de todos modos

## Save: refusals and progress

save-no-document = Abre un PDF antes de guardarlo.
save-not-editable = Este documento no se puede guardar como un PDF editable.
save-chooser-title = Guardar PDF
save-cancelled = Guardado cancelado.
save-busy = Guardando el PDF...
save-done = PDF guardado y vuelto a abrir.
save-failed = No se pudo guardar el PDF: { $error }

## Preview refresh after an edit

preview-refreshing = Actualizando la vista previa...
preview-failed = No se pudo actualizar la vista previa: { $error }

## Protect: password dialog, hints and progress

protect-no-document = Abre un PDF antes de protegerlo.
protect-not-protectable = Este documento no se puede proteger.
protect-dialog-title = Proteger con contraseña
protect-open-label = Contraseña para abrir el documento
protect-permissions-label = Contraseña de permisos
protect-permissions-tooltip = Determina qué pueden hacer los lectores con el documento una vez abierto y levanta esas restricciones para quien la tenga. No puede ser igual que la contraseña de apertura.
protect-cancel = Cancelar
protect-accept = Proteger
protect-open-missing = Introduce la contraseña que pedirá el documento al abrirlo.
protect-permissions-missing = Introduce la contraseña de permisos.
protect-same-passwords = Las dos contraseñas deben ser distintas. Si fueran iguales, cualquiera que pueda abrir el documento podría cambiar también lo que permite.
protect-chooser-title = Guardar el PDF protegido
protect-cancelled = Protección cancelada.
protect-busy = Protegiendo el PDF...
protect-done = PDF protegido, guardado y vuelto a abrir.
protect-failed = No se pudo proteger el PDF: { $error }

## Compress: refusals, dialog, presets and sizes

compress-no-document = Abre un PDF antes de comprimirlo.
compress-not-editable = Este documento no se puede guardar como un PDF editable.
compress-dialog-title = Comprimir PDF
compress-current-size = Este archivo ocupa { $size } en el disco.
compress-preset-lossless = Sin pérdida
compress-preset-lossless-description = Solo reorganiza el archivo. Todas las páginas se ven exactamente igual que ahora.
compress-preset-balanced = Equilibrado
compress-preset-balanced-description = Además, reduce las imágenes a 150 DPI. En pantalla no se aprecia la diferencia y en el disco ocupa bastante menos.
compress-preset-small = Pequeño
compress-preset-small-description = Además, reduce las imágenes a 96 DPI. Para cuando el archivo tiene que caber en algún sitio y los píxeles importan menos que eso.
compress-cancel = Cancelar
compress-accept = Comprimir
compress-writing = Escribiendo el PDF comprimido...
compress-size-megabytes = { $whole },{ $tenth } MB
compress-size-kilobytes = { $size } kB
compress-size-bytes =
    { $count ->
        [one] { $count } byte
       *[other] { $count } bytes
    }

## Compress: results

compress-refusals = No se pudo hacer todo: { $reasons }.
compress-reduced = { $before } sin comprimir → { $after } comprimido ({ $percent } % menos). Elige dónde guardarlo.
compress-no-gain = No hay nada que ganar: comprimido, este documento ocupa los mismos { $size } que al guardarlo sin comprimir, así que no se ha escrito ningún archivo.
compress-written = PDF comprimido guardado en { $path } ({ $before } → { $after }, { $percent } % menos).
compress-chooser-title = Guardar el PDF comprimido
compress-cancelled = Compresión cancelada.
compress-busy = Comprimiendo el PDF...
compress-done = PDF comprimido.
compress-failed = No se pudo comprimir el PDF: { $error }

## Split: refusals, dialog, folder and results

split-no-document = Abre un PDF antes de dividirlo.
split-no-pages = Este documento no tiene páginas que dividir.
split-one-page = Un documento de una sola página no se puede dividir.
split-not-editable = Este documento no se puede dividir como un PDF editable.
split-document-changed = El documento cambió mientras se elegían los cortes. Inténtalo de nuevo.
split-dialog-title = Dividir PDF
split-cuts-label = Dividir después de la página
split-range-hint = Este documento tiene { $total } páginas. Cada corte empieza un archivo nuevo.
split-cancel = Cancelar
split-accept = Dividir
split-not-local-folder = La ubicación seleccionada no es una carpeta local.
split-check-failed = No se pudo comprobar si { $name } ya existe: { $error }
split-replace-title =
    { $count ->
        [one] ¿Reemplazar { $count } PDF existente?
       *[other] ¿Reemplazar { $count } PDF existentes?
    }
split-replace-detail =
    { $count ->
        [one] Al dividir en esta carpeta se escriben { $total } archivos con el nombre del documento, y { $count } de ellos ya existe allí.
       *[other] Al dividir en esta carpeta se escriben { $total } archivos con el nombre del documento, y { $count } de ellos ya existen allí.
    }
split-replace = Reemplazar
split-part-failed = No se pudo escribir { $name }: { $error }
split-summary = PDF dividido en { $count } archivos en { $folder }.
split-signed-note = El documento original está firmado, así que la firma de cada parte ya no es válida.
split-chooser-title = Dividir el PDF en una carpeta
split-cancelled = División cancelada.
split-busy = Dividiendo el PDF...
split-done = PDF dividido.
split-failed = No se pudo dividir el PDF: { $error }

## Extract: refusals, dialog and results

extract-no-document = Abre un PDF antes de extraer páginas de él.
extract-no-pages = Este documento no tiene páginas que extraer.
extract-not-editable = No se pueden extraer páginas de este documento como un PDF editable.
extract-document-changed = El documento cambió mientras se elegía el destino. Inténtalo de nuevo.
extract-empty-selection = Escribe qué páginas quieres extraer, por ejemplo 1-3,7.
extract-dialog-title = Extraer páginas
extract-pages-label = Páginas que extraer
extract-range-hint =
    { $count ->
        [one] Este documento tiene { $count } página.
       *[other] Este documento tiene { $count } páginas.
    }
extract-cancel = Cancelar
extract-accept = Extraer
extract-summary =
    { $count ->
        [one] Se ha extraído { $count } página en { $path }.
       *[other] Se han extraído { $count } páginas en { $path }.
    }
extract-signed-note = El documento original está firmado, así que la firma del PDF extraído ya no es válida.
extract-chooser-title = Guardar el PDF extraído
extract-cancelled = Extracción cancelada.
extract-busy = Extrayendo páginas...
extract-done = Páginas extraídas.
extract-failed = No se pudieron extraer las páginas: { $error }

## Opening documents: chooser, progress, results and page navigation

open-chooser-title = Abrir PDF
open-accept = Abrir
open-not-local-file = La ubicación seleccionada no es un archivo local.
open-window-unavailable = La ventana de la aplicación no está disponible.
open-blank-failed = No se pudo crear un PDF en blanco.
open-busy = Abriendo el PDF...
open-failed = No se pudo abrir el PDF: { $error }
open-no-pages = El PDF no contiene páginas.
open-go-to-page = Ir a la página { $page }
open-name-fallback = PDF abierto
open-name-sample = Documento de ejemplo
open-name-untitled = Documento sin título

## Opening documents: unsaved-changes prompt (before opening another document
## or closing the window)

open-unsaved-title = Cambios sin guardar
open-unsaved-detail-opening = El documento abierto tiene cambios que no se han guardado. Si abres otro documento, se descartarán.
open-unsaved-detail-closing = El documento abierto tiene cambios que no se han guardado. Si cierras Vitela, se descartarán.
open-unsaved-cancel = Cancelar
open-unsaved-discard = Descartar
open-unsaved-save = Guardar
open-unsaved-kept = Se han conservado los cambios sin guardar.

## Opening documents: password prompt for an encrypted PDF

open-password-title = Se necesita una contraseña
open-password-cancel = Cancelar
open-password-open = Abrir
open-password-busy = Abriendo el PDF protegido con contraseña...
open-password-waiting = Esperando la contraseña del documento.
open-password-wrong = La contraseña no es correcta. Inténtalo de nuevo.
open-password-cancelled = Se canceló la contraseña.

## Export images: refusals, dialog, folder and results
# Page-range parsing errors come from the core and stay English.

export-no-document = Abre un PDF antes de exportarlo.
export-no-pages = El PDF no tiene páginas que exportar.
export-document-no-pages = Este documento no tiene páginas que exportar.
export-dialog-title = Exportar páginas como imágenes
export-pages-label = Páginas
export-all-pages = Todas las páginas
export-current-page = Página actual
export-custom-pages = Páginas
export-format-label = Formato
export-resolution-label = Resolución (DPI)
export-cancel = Cancelar
export-accept = Exportar
export-page-too-large = La página { $page } es demasiado grande para generarla a { $dpi } DPI. Elige una resolución más baja.
export-cancelled = Exportación cancelada.
export-chooser-title = Exportar las imágenes a una carpeta
export-not-local-folder = La ubicación seleccionada no es una carpeta local.
export-busy =
    { $count ->
        [one] Exportando { $count } página…
       *[other] Exportando { $count } páginas…
    }
export-unfinished = La exportación no terminó.
export-summary =
    { $count ->
        [one] Se exportó { $count } página a { $folder }.
       *[other] Se exportaron { $count } páginas a { $folder }.
    }
export-prepare-failed = No se pudo preparar el documento para exportarlo: { $error }
export-page-failed = No se pudo exportar la página { $page }: { $error }
export-write-failed = No se pudo escribir { $name }: { $error }

## Add watermark: refusals and dialog
# Page-range parsing errors come from the core and stay English.

watermark-no-document = Abre un PDF antes de añadir una marca de agua.
watermark-cancelled = Marca de agua cancelada.
watermark-dialog-title = Añadir marca de agua
watermark-kind-label = Sello
watermark-kind-text = Texto
watermark-kind-picture = Imagen
watermark-text-label = Texto
# Pre-filled in the text entry and stamped into the PDF as typed.
watermark-default-text = BORRADOR
watermark-size-label = Tamaño (pt)
watermark-choose-picture = Elegir imagen…
watermark-no-picture = No se ha elegido ninguna imagen
watermark-picture-label = Imagen (PNG o JPEG)
watermark-scale-label = Tamaño (% de la página)
watermark-pages-label = Páginas
watermark-all-pages = Todas las páginas
watermark-current-page = Página actual
watermark-custom-pages = Páginas
watermark-opacity-label = Opacidad (%)
watermark-angle-label = Ángulo (grados)
watermark-position-label = Posición
watermark-over = Sobre el contenido de la página
watermark-under = Detrás del contenido de la página
watermark-cancel = Cancelar
watermark-accept = Añadir marca de agua
watermark-chooser-title = Elegir la imagen de la marca de agua
watermark-chooser-accept = Elegir
watermark-not-local-file = La ubicación seleccionada no es un archivo local.
watermark-read-failed = No se pudo leer { $path }: { $error }

## Add watermark: validation and results

watermark-no-pages = Este documento no tiene páginas que marcar.
watermark-text-required = Escribe el texto que se estampará en las páginas.
watermark-picture-required = Elige una imagen PNG o JPEG para estamparla en las páginas.
watermark-no-such-page = Este documento no tiene la página { $page }.
watermark-font-gap = La fuente de la marca de agua no puede mostrar «{ $character }». Usa letras latinas, dígitos y signos de puntuación.
watermark-invalid-picture = No se pudo leer ese archivo como una imagen PNG o JPEG. Elige otro.
watermark-not-recordable = No se puede añadir esta marca de agua a estas páginas.
watermark-summary =
    { $count ->
        [one] Marca de agua añadida a { $count } página.
       *[other] Marca de agua añadida a { $count } páginas.
    }

## Shared refusals: document permissions and editable-model availability

refusal-text-forbidden = Este documento no permite copiar ni extraer su texto.
refusal-text-unreadable = No se pudieron leer los permisos de este documento, así que no se puede extraer su texto.
refusal-annotation-forbidden = Este documento no permite cambiar sus anotaciones.
refusal-annotation-model-unavailable = No se pudo preparar este documento para cambiar sus anotaciones.
refusal-content-forbidden = Este documento no permite cambiar su contenido.
refusal-content-model-unavailable = No se pudo preparar este documento para cambiar su contenido.
refusal-assembly-forbidden = Este documento no permite añadir, quitar ni reordenar sus páginas.
refusal-assembly-unreadable = No se pudieron leer los permisos de este documento, así que no se pueden cambiar sus páginas.
refusal-rewrite-needs-passwords = Cambiar las páginas de este documento cifrado reescribe el archivo completo, lo que requiere sus contraseñas de usuario y de propietario. Vuelve a abrirlo con ambas para continuar.
refusal-rewrite-unsupported-encryption = El cifrado de este documento todavía no se puede reproducir al reescribir el archivo, así que no se pueden cambiar sus páginas.
refusal-page-no-longer-present = Esa página ya no forma parte del documento.

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
annotate-edit-text = Editar texto
annotate-delete = Eliminar
annotate-color-title = Elegir el color de la anotación

## Annotate: status messages and refusals

# $tool (String) - the localized tool name (annotate-tool-*).
annotate-tool-armed = Herramienta { $tool } activada: arrastra en una página para colocar la anotación.
annotate-tool-disarmed = Herramienta { $tool } desactivada.
annotate-markup-applied = Anotación ({ $tool }) aplicada al texto seleccionado. Los cambios están pendientes de guardar.
annotate-added = Anotación añadida ({ $tool }). Los cambios están pendientes de guardar.
annotate-not-markup = La herramienta { $tool } no marca texto.
# $error (String) - technical detail from the image decoder, kept verbatim.
annotate-image-unusable = No se pudo usar la imagen: { $error }
annotate-stamp-added = Sello de imagen añadido. Los cambios están pendientes de guardar.
annotate-undone = Cambio deshecho.
annotate-redone = Cambio rehecho.
annotate-undone-pending = Cambio deshecho. Los cambios están pendientes de guardar.
annotate-redone-pending = Cambio rehecho. Los cambios están pendientes de guardar.
annotate-edited = Anotación editada. Los cambios están pendientes de guardar.
annotate-deleted = Anotación eliminada. Los cambios están pendientes de guardar.
# $id (Number) - the annotation's internal number.
annotate-selected = Anotación { $id } seleccionada.
annotate-moved = Anotación movida. Los cambios están pendientes de guardar.
annotate-resized = Anotación redimensionada. Los cambios están pendientes de guardar.
annotate-color-stale = La selección de color ya no está vigente.
annotate-restyled = Estilo de la anotación cambiado. Los cambios están pendientes de guardar.
annotate-ink-needs-drag = Arrastra para dibujar una anotación de tinta.
annotate-note-needs-text = Escribe el texto de la nota antes de añadirla.
annotate-no-document = Abre un PDF antes de editar las anotaciones.
annotate-selection-gone = La anotación seleccionada ya no existe.
annotate-select-first = Selecciona primero una anotación.
annotate-drag-unsupported = Esa anotación no se puede cambiar de esta forma.

## Annotate: note and text box dialogs

annotate-note-canceled = Colocación de la nota cancelada.
annotate-note-no-dialog = No se pudo abrir el cuadro de diálogo de la nota.
# $page (Number) - one-based page number.
annotate-note-title = Añadir nota — página { $page }
annotate-note-text = Texto de la nota
annotate-dialog-cancel = Cancelar
annotate-dialog-add = Añadir
annotate-dialog-save = Guardar
annotate-dialog-text = Texto
annotate-note-added = Nota de texto añadida. Los cambios están pendientes de guardar.
annotate-text-box-add-canceled = Colocación del cuadro de texto cancelada.
annotate-text-box-edit-canceled = Edición del cuadro de texto cancelada.
annotate-text-box-needs-text = Escribe el texto del cuadro antes de añadirlo.
annotate-text-box-too-small = Ese cuadro de texto es demasiado pequeño.
annotate-text-box-select-first = Selecciona primero un cuadro de texto.
annotate-text-box-document-changed = El documento cambió mientras el cuadro de diálogo estaba abierto; no se añadió nada.
annotate-text-box-no-dialog = No se pudo abrir el cuadro de diálogo del cuadro de texto.
annotate-text-box-unchanged = El texto no ha cambiado.
# $character (String) - the first character the text box font cannot show.
annotate-text-box-encoding-gap = El carácter '{ $character }' no se puede usar en un cuadro de texto: se dibuja en Helvetica, que solo cubre texto de Europa occidental. Elimínalo y vuelve a intentarlo.
# $page (Number) - one-based page number.
annotate-text-box-add-title = Añadir cuadro de texto — página { $page }
annotate-text-box-edit-title = Editar cuadro de texto
annotate-text-box-added = Cuadro de texto añadido. Los cambios están pendientes de guardar.
annotate-text-box-edited = Cuadro de texto editado. Los cambios están pendientes de guardar.

## Edit PDF: content edit results and refusals

edit-no-window = La ventana de la aplicación no está disponible.
edit-text-deleted = Texto eliminado.
edit-text-moved-unsaved = Este texto se movió y aún no se ha guardado: guarda y vuelve a abrir el documento antes de reescribirlo.
edit-text-edit-unavailable = Este texto forma parte de un cambio sin guardar que ya no está disponible: guarda y vuelve a abrir el documento antes de volver a editarlo.
edit-text-updated = Texto actualizado.
edit-text-inserted = Texto insertado.
edit-insertion-not-deletable = Este texto aún no se ha guardado: deshaz la inserción para quitarlo.
edit-cancelled = Edición cancelada.
edit-text-retyped-unsaved = Este texto tiene un cambio sin guardar: guarda el documento antes de moverlo.
edit-no-text-there = No hay texto que editar ahí.
edit-open-to-edit = Abre un PDF para editar su contenido.
edit-open-to-copy = Abre un PDF para copiar su contenido.
edit-text-moved = Texto movido.
edit-image-moved = Imagen movida.
edit-image-resized = Imagen redimensionada.
edit-image-deleted = Imagen eliminada.
edit-replace-image-title = Reemplazar imagen
edit-replace-image-accept = Reemplazar
edit-not-local-file = La ubicación seleccionada no es un archivo local.
# $path (String) - file path; $error (String) - technical detail, kept verbatim.
edit-read-failed = No se pudo leer { $path }: { $error }
edit-image-replaced = Imagen reemplazada.
edit-image-files = Archivos de imagen
edit-insert-image-title = Insertar imagen
edit-insert-image-accept = Insertar
# $error (String) - technical detail from the image decoder, kept verbatim.
edit-image-unusable = No se pudo usar la imagen: { $error }
edit-image-inserted = Imagen insertada.
edit-shape-moved = Forma movida.
edit-shape-resized = Forma redimensionada.
edit-shape-deleted = Forma eliminada.

## Edit PDF: multi-select and content clipboard

# $count (Number) - selected items.
edit-multi-selected =
    { $count ->
        [one] { $count } elemento seleccionado: arrástralo para moverlo, Supr lo elimina, Esc borra la selección.
       *[other] { $count } elementos seleccionados: arrástralos para moverlos, Supr los elimina, Esc borra la selección.
    }
edit-multi-nothing = No hay nada que seleccionar ahí.
edit-multi-moved = Selección movida.
edit-multi-deleted = Selección eliminada.
edit-multi-cleared = Selección borrada.
# $count (Number) - items placed on the content clipboard.
edit-clipboard-copied =
    { $count ->
        [one] { $count } elemento copiado.
       *[other] { $count } elementos copiados.
    }
edit-clipboard-cut =
    { $count ->
        [one] { $count } elemento cortado.
       *[other] { $count } elementos cortados.
    }
edit-clipboard-paste-off-page = El contenido solo se puede pegar en una página del documento.
edit-clipboard-pasted = Pegado.

## Forms: toolbar, field kinds and fill panel

forms-kind-text = Campo de texto
forms-kind-checkbox = Casilla de verificación
forms-kind-radio-group = Grupo de opciones
forms-kind-dropdown = Lista desplegable
forms-edit-mode = Editar formularios
forms-color = Color
forms-delete-field = Eliminar campo
forms-fill-placeholder = Abre un PDF con campos de formulario para rellenarlos o coloca uno en el modo Editar formularios.
forms-heading-fields = Campos de formulario
forms-heading-style = Estilo del campo
forms-heading-fill = Rellenar campos
forms-none-choice = (ninguna)
forms-unsupported-field = Campo no compatible
forms-field-updated = Campo de formulario actualizado.

## Forms: edit mode and field results

forms-value-set = Valor del campo establecido. Los cambios están pendientes de guardar.
forms-renamed = Campo renombrado. Los cambios están pendientes de guardar.
forms-selection-gone = El campo de formulario seleccionado ya no existe.
forms-mode-armed = Edición de formularios activada: elige un tipo de campo para colocarlo o haz clic en un campo existente para seleccionarlo.
forms-mode-disarmed = Edición de formularios desactivada.
# $kind (String) - the localized field kind (forms-kind-*).
forms-kind-armed = { $kind }: arrastra en la página para colocar el campo.
forms-mode-armed-select = Edición de formularios activada: haz clic en un campo existente para seleccionarlo.
forms-no-document = Abre un PDF antes de editar los campos de formulario.
forms-deleted = Campo eliminado. Guarda para conservar el cambio o deshazlo para recuperarlo.
forms-added = Campo añadido ({ $kind }). Los cambios están pendientes de guardar.
forms-moved = Campo movido. Los cambios están pendientes de guardar.
forms-resized = Campo redimensionado. Los cambios están pendientes de guardar.
forms-restyled = Estilo del campo cambiado. Los cambios están pendientes de guardar.
forms-color-title = Elegir el color del campo
forms-color-stale = La selección de color ya no está vigente.

## Viewer: dropping and pasting files

drop-not-local = Solo se pueden soltar archivos PDF e imágenes locales.
paste-open-first = Abre un PDF antes de pegar una imagen.
paste-no-image = El portapapeles no contiene una imagen de mapa de bits.
drop-page-not-ready = La página aún no está lista para colocar la imagen.
drop-loading = Cargando el archivo soltado...
drop-unsupported = El archivo soltado no es un PDF ni una imagen compatible.
# $error (String) - technical detail from the file system, kept verbatim.
drop-read-failed = No se pudo leer el archivo soltado: { $error }
drop-image-on-page = Suelta la imagen directamente sobre una página de un PDF abierto.

## Viewer: printing

print-open-first = Abre un PDF antes de imprimir.
print-preparing = Preparando la impresión…
# $error (String) - technical detail, kept verbatim.
print-prepare-failed = No se pudo preparar el documento para imprimirlo: { $error }
print-prepare-stopped = No se pudo preparar el documento para imprimirlo.
print-no-pages = El PDF no tiene páginas que imprimir.
print-failed = No se pudo imprimir.
# $error (String) - technical detail from GTK, kept verbatim.
print-error = No se pudo imprimir: { $error }

## Viewer: page rendering status

# $first, $last, $total (Number) - one-based visible page range and page count.
render-showing-pages = Mostrando las páginas { $first }-{ $last } de { $total }.
# $page (Number) - one-based page number.
render-page-too-large = La página { $page } no se puede mostrar de forma segura con este tamaño.
# $page, $total (Number) - one-based page and page count; $error (String) - renderer detail, kept verbatim.
render-page-failed = No se pudo mostrar la página { $page } de { $total }: { $error }
render-sharpen-failed = No se pudo mostrar con más nitidez la página { $page } de { $total }: { $error }
# $page (Number) - one-based page; $returned, $requested (Number) - tile counts.
render-tile-mismatch = La página { $page } devolvió { $returned } mosaicos de los { $requested } solicitados.

## Viewer: text selection

selection-copy-empty = Selecciona texto antes de copiar.
# $count (Number) - characters copied.
selection-copied =
    { $count ->
        [one] { $count } carácter copiado al portapapeles.
       *[other] { $count } caracteres copiados al portapapeles.
    }

## Command palette (B27)

palette-title = Paleta de comandos
palette-search-placeholder = Buscar comandos…
palette-no-results = Ningún comando coincide.
palette-nothing-to-undo = No hay nada que deshacer.
palette-nothing-to-redo = No hay nada que rehacer.

## Command palette search synonyms (B27 T-231): space-separated words a
## user might type for each command, matched alongside its label.

palette-mode-home-keywords = inicio portada recientes archivos
palette-mode-edit-keywords = cambiar modificar contenido texto imagen
palette-mode-annotate-keywords = comentar comentario revisar notas marcar
palette-mode-organize-keywords = páginas página ordenar reordenar borrar eliminar mover miniaturas
palette-mode-sign-keywords = firma certificado rellenar formulario
palette-mode-protect-keywords = contraseña cifrar encriptar bloquear seguridad permisos
palette-file-open-keywords = cargar documento examinar
palette-file-new-keywords = crear vacío documento
palette-file-save-keywords = guardar copia conservar descargar
palette-file-print-keywords = impresora papel
palette-edit-undo-keywords = revertir volver atrás cancelar
palette-edit-redo-keywords = repetir
palette-edit-find-keywords = buscar búsqueda localizar encontrar texto
palette-annotate-highlight-keywords = resaltar marcador marcar color amarillo
palette-annotate-underline-keywords = subrayar línea marcar
palette-annotate-strikeout-keywords = tachar línea
palette-annotate-ink-keywords = dibujar lápiz bolígrafo mano alzada
palette-annotate-note-keywords = comentario comentar adhesiva
palette-annotate-text-box-keywords = escribir texto libre etiqueta
palette-annotate-shape-keywords = rectángulo cuadrado círculo elipse línea flecha
palette-annotate-stamp-keywords = aprobado timbre imagen
palette-forms-text-keywords = formulario campo entrada
palette-forms-checkbox-keywords = marcar tilde formulario campo
palette-forms-radio-group-keywords = opción botón elegir formulario campo
palette-forms-dropdown-keywords = lista seleccionar desplegable formulario campo opciones
palette-document-export-images-keywords = exportar imagen imágenes png jpeg foto convertir
palette-document-watermark-keywords = marca agua sello logo fondo
palette-document-compress-keywords = reducir tamaño comprimir achicar optimizar
palette-pages-add-pdfs-keywords = unir combinar fusionar importar insertar
palette-pages-extract-keywords = páginas página extraer separar selección
palette-pages-split-keywords = páginas página separar partir cortar
palette-sign-certificate-file-keywords = firma firmar digital pfx p12
palette-sign-token-keywords = firma firmar tarjeta inteligente digital
palette-sign-system-certificate-keywords = firma firmar digital instalado
palette-sign-draw-keywords = firma firmar manuscrita rúbrica iniciales
