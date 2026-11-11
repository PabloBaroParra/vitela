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
