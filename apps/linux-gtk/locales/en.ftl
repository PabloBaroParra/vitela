# English strings for the Linux GTK shell. This file is the reference:
# es.ftl must define exactly the same message ids (a unit test enforces it).

## Home: header

home-search-placeholder = Search recent files and tools
home-open-file = Open file
home-open-file-tooltip = Open a PDF (Ctrl+O)

## Home: welcome and drop zone

home-welcome-title = Welcome to Vitela
home-welcome-subtitle = Your fast, private PDF workspace.
home-start-task = Start a new task
home-drop-prompt = Drag and drop a PDF here
home-drop-hint = or click anywhere in this area to open one
home-select-file = Select file
home-drop-not-local = Only local files can be dropped here.
home-drop-not-pdf = Only PDF files can be opened from the Home screen.

## Home: recent documents

home-recent-title = Recent
home-recent-today = Today
home-recent-yesterday = Yesterday
home-recent-earlier = Earlier
home-recent-empty = No recent documents yet. Open a PDF and it will show up here.
home-recent-no-match = No recent document matches that search.
# $name (String) - the document's file name.
home-recent-open-card = Open { $name }
home-recent-remove = Remove from Recent
# $name (String) - the document's file name.
home-recent-remove-failed = Could not remove { $name } from Recent.
# $time (String) - the time of day, already formatted (e.g. 10:45).
home-recent-opened-today = Opened today, { $time }
# $time (String) - the time of day, already formatted (e.g. 10:45).
home-recent-opened-yesterday = Opened yesterday, { $time }
# $date (String) - day and abbreviated month, already formatted (e.g. 7 Nov).
home-recent-opened-on = Opened { $date }
# $count (Number) - pages in the document.
home-recent-page-count =
    { $count ->
        [one] { $count } page
       *[other] { $count } pages
    }

## Home: tool grid

home-tools-title = Tools
home-tool-unavailable = Not available yet
home-tool-edit = Edit
home-tool-edit-description = Retype text and replace images
home-tool-annotate = Annotate
home-tool-annotate-description = Highlight, draw, and add notes
home-tool-sign = Sign
home-tool-sign-description = Sign with a certificate, card, or token
home-tool-organize = Organize
home-tool-organize-description = Reorder and delete pages
home-tool-compress = Compress
home-tool-compress-description = Write a smaller copy of the file
home-tool-protect = Protect
home-tool-protect-description = Require a password to open the document

## Home: quick actions

home-quick-actions-title = Quick actions
home-quick-new-blank = New blank PDF
home-quick-open = Open file…
home-quick-sample = Open the sample

## Home: keyboard shortcuts

home-shortcuts-title = Keyboard shortcuts
home-shortcut-open = Open
home-shortcut-new = New
home-shortcut-save = Save
home-shortcut-find = Find
home-shortcut-print = Print

## Home: version footer

# $version (String) - the release version, or home-version-dev.
home-version = Vitela { $version }
home-version-dev = dev

## Persistent navigation rail

rail-sections = Vitela sections
rail-home = Home
rail-recent = Recent
rail-files = My files
rail-edit = Edit PDF
rail-annotate = Annotate
rail-sign = Sign
rail-organize = Organize pages
rail-protect = Protect

## Editor: toolbar groups (accessible names)

editor-group-document = Document
editor-group-output = Output
editor-group-history = History
editor-group-position = Position
editor-group-zoom = Zoom
editor-group-fit = Fit
editor-group-panels = Panels
editor-group-find = Find

## Editor: document commands, navigation and search

editor-open = Open PDF
editor-open-short = Open
editor-open-tooltip = Open PDF (Ctrl+O)
editor-sample = Open sample
editor-sample-plain = Vitela sample
# Keep the sample passwords unchanged: users enter them to open the files.
editor-sample-aes128 = AES-128 sample (user-aes-pass)
editor-sample-rc4128 = RC4-128 sample (user-rc4-pass)
editor-save = Save as
editor-save-tooltip = Save as (Ctrl+S)
editor-print = Print
editor-print-tooltip = Print (Ctrl+P)
editor-export = Export images
editor-export-tooltip = Export pages as images
editor-undo = Undo
editor-redo = Redo
editor-page-position-tooltip = Current page / total pages
editor-zoom-out = Zoom out
editor-zoom-in = Zoom in
editor-fit-width = Fit width
editor-fit-page = Fit page
editor-pages = Pages
editor-tools = Tools
editor-find = Find in document
editor-find-tooltip = Find in document (Ctrl+F)
editor-search-document = Search document
editor-find-previous = Previous match
editor-find-next = Next match
editor-find-hint = Case-sensitive. Press Enter to search.
editor-idle-status = Choose a PDF file to view, or open the built-in sample.
editor-tools-panel = Tools and properties

## Search: status messages

search-enter-text = Enter text to find.
search-open-first = Open a PDF before searching.
# $query (String) - the user's search text, kept verbatim.
search-running = Searching for "{ $query }"...
search-no-matches = No matches for "{ $query }".
# $error (String) - technical detail from the renderer, kept verbatim.
search-failed = Could not search: { $error }
# $index, $total (Number) - one-based match index and total matches.
search-match = Match { $index } of { $total } for "{ $query }".

## Tools panel: navigation (page identifiers stay language-independent)

tools-tab-edit = Edit
tools-tab-annotate = Annotate
tools-tab-comments = Comments
tools-tab-fill-sign = Fill & Sign
tools-annotations = Annotations
tools-comments-unavailable = Comments aren't available in this shell yet.

## Document properties: labels and edit results

metadata-heading = Document properties
metadata-pages = Pages
metadata-title = Title
metadata-author = Author
metadata-subject = Subject
metadata-keywords = Keywords
metadata-creator = Creator
metadata-producer = Producer
metadata-created = Created
metadata-modified = Modified
# Display hint only: the accepted date/time format stays YYYY-MM-DD HH:MM:SS.
metadata-date-placeholder = YYYY-MM-DD HH:MM:SS
metadata-open-first = Open a PDF before editing its properties.
metadata-unchanged = No change to record.
metadata-updated = Document properties updated. Changes are pending save.
metadata-invalid-date = Invalid date, reverted: { $error }

## Document properties: date validation (field names are localized arguments)

metadata-year = year
metadata-month = month
metadata-day = day
metadata-hour = hour
metadata-minute = minute
metadata-second = second
metadata-date-missing = expected YYYY-MM-DD, missing { $field }
metadata-date-not-number = expected YYYY-MM-DD, "{ $field }" is not a number
metadata-date-extra = expected YYYY-MM-DD, found extra "-"-separated parts
metadata-year-overflow = year { $year } does not fit
metadata-time-missing = expected HH:MM or HH:MM:SS, missing { $field }
metadata-time-not-number = expected HH:MM or HH:MM:SS, "{ $field }" is not a number
metadata-time-extra = expected HH:MM or HH:MM:SS, found extra ":"-separated parts
metadata-component-range = { $field } { $value } is out of range ({ $min }-{ $max })

## Edit PDF: panel controls, tooltips and gesture hints

edit-heading = Edit PDF
edit-description = Change the text, images and shapes already on the page.
edit-card-text = Text
edit-card-images = Images
edit-card-shapes = Shapes
edit-card-pages = Pages
edit-mode = Edit content
edit-mode-tooltip = Turn on content editing, then click text on the page to select it
edit-insert-text = Insert text
edit-insert-text-tooltip = Click the page to place a new text box
edit-delete-text = Delete text
edit-delete-text-tooltip = Remove the selected text, or the text being retyped, from the page
edit-insert-image = Insert image
edit-insert-image-tooltip = Click the page to insert a picture
edit-replace-image = Replace image
edit-replace-image-tooltip = Swap the selected image for a file on disk
edit-delete-image = Delete image
edit-delete-image-tooltip = Remove the selected image from the page
edit-delete-shape = Delete shape
edit-delete-shape-tooltip = Remove the selected line, border or box from the page
edit-watermark = Add watermark
edit-watermark-tooltip = Stamp the same text across the pages you choose
edit-watermark-hint = A watermark is painted into the pages themselves and is one undo step.
edit-no-document = Open a PDF to edit the text and images on its pages.
edit-no-text-selected = Click text to select it; double-click to retype it in place, or drag it to move it.
edit-text-selected = Text selected — drag to move it, press Delete to remove it, double-click or F2 to retype it.
edit-text-editing = Retyping text — press Enter to keep it or Esc to cancel, or delete it.
edit-no-image-selected = Click an image on the page to select it.
edit-image-selected = Image selected — replace or delete it.
edit-image-no-replace = Image selected — move, resize or delete it. It cannot be replaced: its current encoding cannot be read back, so the swap could not be undone.
edit-image-pending = This image has an unsaved edit — move, resize or delete it, or save before replacing it.
edit-no-shape-selected = Click a line, border or shaded box on the page to select it.
edit-shape-selected = Shape selected — drag it, drag a corner to resize it, or press Delete.

## Edit PDF: mode activation messages

edit-mode-armed = Edit content armed — click text to select it, double-click to retype it.
edit-mode-disarmed = Edit content disarmed.
edit-insert-text-armed = Insert text armed — click the page to place a new text box.
edit-insert-image-armed = Insert image armed — click the page to insert a picture.

## Signing: certificate section, choosers and unlock prompts
# Adapter and module error details arrive untranslated as { $error }.

sign-heading = Signing
sign-choose-pfx = Choose signing certificate (.pfx)…
sign-use-token = Use card or token…
sign-use-nss = Use a certificate from this computer…
sign-signed-indicator = ✓ This document is digitally signed.
sign-open = Open
sign-cancel = Cancel
sign-unlock = Unlock
sign-not-local-file = The selected location is not a local file.
sign-not-local-folder = The selected location is not a local folder.
sign-pfx-filter = PKCS#12 certificates
sign-pfx-chooser-title = Choose signing certificate
sign-pfx-password-title = Certificate password required
sign-pfx-reading = Reading certificate file...
sign-pfx-no-identities = This certificate file has no usable signing identities.
sign-pfx-waiting = Waiting for the certificate password.
sign-pfx-wrong-password = The password is incorrect, or the file is not a valid PKCS#12 certificate.
sign-pfx-read-failed = Could not read the certificate file: { $error }
sign-certificate-cancelled = Certificate selection cancelled.
sign-found-identities =
    { $count ->
        [one] Found { $count } signing identity: { $names }
       *[other] Found { $count } signing identities: { $names }
    }
sign-pkcs11-filter = PKCS#11 modules
sign-pkcs11-chooser-title = Choose PKCS#11 module
sign-pkcs11-pin-title = Card or token PIN required
sign-pkcs11-reading = Reading card or token...
sign-pkcs11-waiting = Waiting for the token PIN.
sign-pkcs11-no-identities = No signing identities were found. Check the PIN and that the card or token holds a certificate.
sign-pkcs11-load-failed = Could not load the PKCS#11 module: { $error }
sign-pkcs11-cancelled = Card or token selection cancelled.
sign-nss-filter = NSS modules
sign-nss-module-chooser-title = Choose the NSS certificate module (libsoftokn3.so)
sign-nss-profile-chooser-title = Choose the certificate database folder
sign-nss-password-title = Certificate database password
sign-nss-password-hint = Leave this blank if your certificates have no password set.
sign-nss-reading = Reading certificate database...
sign-nss-waiting = Waiting for the certificate database password.
sign-nss-no-identities = No signing certificates were found. Check the password and that this computer has certificates imported.
sign-nss-load-failed = Could not load the certificate database: { $error }
sign-nss-invalid-path = The certificate database path { $path } cannot be used.

## Signing: identity picker and signing refusals

sign-picker-title = Choose a signing identity
sign-picker-prompt = Sign this document with:
sign-picker-sign = Sign
sign-picker-none-chosen = Choose a signing identity.
sign-cancelled = Signing cancelled.
sign-no-document = Open a PDF before signing.
sign-unsaved-changes = Save your changes before signing this document.
sign-not-signable = This document cannot be signed.

## Drawn signature: entry, pad, saved-signature offer and placement

signature-draw = Draw signature…
signature-draw-tooltip = Draw your signature with the mouse, then click a page to place it as an image
signature-pad-title = Draw your signature
signature-remember = Remember on this computer
signature-clear = Clear
signature-cancel = Cancel
signature-use = Use
signature-offer-title = Your signature
signature-offer-preview = Your saved signature
signature-offer-unshowable = This saved signature can no longer be shown.
signature-draw-new = Draw new
signature-delete = Delete
signature-no-document = Open a PDF before drawing a signature.
signature-unrenderable = The signature could not be turned into an image.
signature-not-remembered = Your signature could not be remembered. Click a page to place it.
signature-forgotten = Your saved signature was deleted from this computer.
signature-armed = Click a page to place your signature.
signature-disarmed = Signature disarmed.
signature-stale = That signature was drawn for another document. Draw it again to place it.
