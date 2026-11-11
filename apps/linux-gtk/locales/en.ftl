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

## Organize: header, view switch and hints

organize-heading = Organize pages
organize-import-progress-name = PDF import progress
organize-cancel-import = Cancel
organize-undo = Undo
organize-redo = Redo
organize-add-pdfs = Add PDFs
organize-extract = Extract
organize-extract-tooltip = Save chosen pages as a new PDF
organize-split = Split
organize-split-tooltip = Cut this PDF into several new PDFs
organize-save = Save
organize-view-documents = Documents
organize-view-pages = Pages
organize-documents-hint = Drag a document to move all of its pages.
organize-pages-hint = Drag a page to reorder it.

## Organize: page and document cards (accessible names and tooltips)

# $name (String) - the file name of the source PDF.
# $part (Number) - which run of that file's pages this card is.
organize-block-title-part = { $name } — Part { $part }
# $first (Number) - one-based position of the block's only page.
organize-block-meta-one = 1 page · { $first }
# $count, $first, $last (Number) - block size and one-based first/last position.
organize-block-meta = { $count } pages · { $first }–{ $last }
# $title, $meta (String) - the card's two lines; $position, $total (Number).
organize-block-name = { $title }, { $meta }, document { $position } of { $total }
organize-block-move-up = Move up: { $name }
organize-block-move-down = Move down: { $name }
organize-block-rotate-left = Rotate left: { $name }
organize-block-rotate-right = Rotate right: { $name }
organize-block-delete = Delete { $name }
organize-blank-pages = Blank pages
organize-blank-page = Blank page
organize-imported-pdf = Imported PDF
organize-page-rotate-left = Rotate page left
organize-page-rotate-right = Rotate page right
organize-page-delete = Delete page

## Organize: page operation results and refusals

organize-no-document = Open a PDF before organizing its pages.
organize-invalid-position = Invalid page position.
organize-page-gone = Page no longer exists.
organize-pages-gone = Those pages no longer exist.
organize-move-page-failed = Could not move the page.
organize-move-document-failed = Could not move the document.
organize-delete-page-failed = Could not delete the page.
organize-delete-document-failed = Could not delete the document.
organize-rotate-page-failed = Could not rotate the page.
organize-rotate-document-failed = Could not rotate the document.
# $from, $to (Number) - one-based page positions.
organize-moved-page = Moved page { $from } to position { $to }.
# $count (Number) - pages moved; $to (Number) - one-based destination.
organize-moved-pages =
    { $count ->
        [one] Moved { $count } page to position { $to }.
       *[other] Moved { $count } pages to position { $to }.
    }
# $page (Number) - one-based page position.
organize-deleted-page = Deleted page { $page }.
organize-deleted-pages =
    { $count ->
        [one] Deleted { $count } page.
       *[other] Deleted { $count } pages.
    }
organize-rotated-page-left = Rotated page { $page } left.
organize-rotated-page-right = Rotated page { $page } right.
organize-rotated-pages-left =
    { $count ->
        [one] Rotated { $count } page left.
       *[other] Rotated { $count } pages left.
    }
organize-rotated-pages-right =
    { $count ->
        [one] Rotated { $count } page right.
       *[other] Rotated { $count } pages right.
    }

## Organize: adding PDFs (chooser, progress, passwords and results)

organize-import-filter = PDF files
organize-import-chooser-title = Add PDFs
organize-import-accept = Add
organize-import-cancelled = PDF import cancelled.
organize-import-changed = PDF import cancelled because the document changed.
organize-import-not-local = Every selected PDF must be a local file.
organize-import-checking = Checking selected PDFs...
# $completed, $total (Number) - PDFs prepared so far, out of the selection.
organize-import-progress = { $completed } of { $total } PDFs
organize-import-worker-stopped = PDF import worker stopped unexpectedly.
organize-import-unnamed = Selected PDF
# $name (String) - file name; $error (String) - technical detail, kept verbatim.
organize-import-failed = Could not import { $name }: { $error }
organize-import-no-copying = Could not import { $name }: the PDF does not permit copying its pages.
organize-import-no-pages = Could not import { $name }: the PDF has no pages.
organize-import-too-many-pages = The selected PDFs have too many pages.
organize-import-source-ids-exhausted = Imported PDF id space is exhausted.
organize-import-page-ids-exhausted = Page id space is exhausted.
organize-import-password-title = Password required
organize-import-password-prompt = Enter the password for { $name }.
organize-import-password-wrong = The password is incorrect. Try again.
organize-import-password-cancel = Cancel import
organize-import-unlock = Unlock
organize-import-password-waiting = Waiting for the password for { $name }.
organize-import-warning-title = Some document-level information will stay behind
organize-import-warning-cancel = Cancel
organize-import-anyway = Import anyway
organize-import-apply-failed = Could not add the selected PDFs.
# $count (Number) - pages imported. English keeps "pages" for every count.
organize-imported-pages = Imported { $count } pages.

## Write chains: shared by Compress, Split and Extract

write-no-save-backing = This document has no save backing.

## Compress: refusals, dialog, presets and sizes
# The destination chooser, its replace prompt and the signature warning are
# shared with Save and stay English for now.

compress-no-document = Open a PDF before compressing it.
compress-not-editable = This document cannot be saved as an editable PDF.
compress-dialog-title = Compress PDF
# $size (String) - the file's size, already formatted (compress-size-*).
compress-current-size = This file is { $size } on disk.
compress-preset-lossless = Lossless
compress-preset-lossless-description = Repack the file only. Every page renders exactly as it does now.
compress-preset-balanced = Balanced
# Keep "150 DPI": a test pins the number against the preset.
compress-preset-balanced-description = Also bring images down to 150 DPI. Visibly unchanged on screen, materially smaller on disk.
compress-preset-small = Small
# Keep "96 DPI": a test pins the number against the preset.
compress-preset-small-description = Also bring images down to 96 DPI. For when the file has to fit through something and the pixels matter less than that.
compress-cancel = Cancel
compress-accept = Compress
compress-writing = Writing the compressed PDF...
# $whole, $tenth (String) - integer and first decimal digit of the size in MB.
compress-size-megabytes = { $whole }.{ $tenth } MB
# $size (String) - the size in kB, already rounded.
compress-size-kilobytes = { $size } kB
# $count (Number) - the size in bytes, under 1000.
compress-size-bytes =
    { $count ->
        [one] { $count } byte
       *[other] { $count } bytes
    }

## Compress: results

# $reasons (String) - the core's refusal sentences, kept verbatim.
compress-refusals = Not everything could be done: { $reasons }.
# $before, $after (String) - formatted sizes; $percent (Number) - whole percent saved.
compress-reduced = { $before } uncompressed → { $after } compressed ({ $percent }% smaller). Choose where to write it.
# $size (String) - formatted size.
compress-no-gain = Nothing to gain: this document compresses to the same { $size } an uncompressed save writes, so no file was written.
# $path (String) - destination path; $before, $after (String); $percent (Number).
compress-written = Compressed PDF written to { $path } ({ $before } → { $after }, { $percent }% smaller).
compress-chooser-title = Save compressed PDF
compress-cancelled = Compression cancelled.
compress-busy = Compressing PDF...
compress-done = PDF compressed.
# $error (String) - technical detail, kept verbatim.
compress-failed = Could not compress PDF: { $error }

## Split: refusals, dialog, folder and results
# Cut-list parsing errors come from the core and stay English.

split-no-document = Open a PDF before splitting it.
split-no-pages = This document has no pages to split.
split-one-page = A document of one page cannot be split.
split-not-editable = This document cannot be split as an editable PDF.
split-document-changed = The document changed while the cuts were being chosen. Try again.
split-dialog-title = Split PDF
split-cuts-label = Split after page
# $total (Number) - pages in the document, always two or more.
split-range-hint = This document has { $total } pages. Each cut starts a new file.
split-cancel = Cancel
split-accept = Split
split-not-local-folder = The selected location is not a local folder.
# $name (String) - part file name; $error (String) - technical detail, kept verbatim.
split-check-failed = Could not check whether { $name } already exists: { $error }
# $count (Number) - part files that already exist in the folder.
split-replace-title =
    { $count ->
        [one] Replace { $count } existing PDF?
       *[other] Replace { $count } existing PDFs?
    }
# $total (Number) - files the split writes; $count (Number) - those that already exist.
split-replace-detail = Splitting into this folder writes { $total } files named after the document, and { $count } of them already exist there.
split-replace = Replace
# $name (String) - part file name; $error (String) - technical detail, kept verbatim.
split-part-failed = { $name } could not be written: { $error }
# $count (Number) - files written, always two or more; $folder (String) - folder path.
split-summary = Split into { $count } PDFs in { $folder }.
split-signed-note = The original document is signed, so the signature in each part no longer verifies.
split-chooser-title = Split PDF into folder
split-cancelled = Split cancelled.
split-busy = Splitting PDF...
split-done = PDF split.
# $error (String) - technical detail, kept verbatim.
split-failed = Could not split PDF: { $error }

## Extract: refusals, dialog and results
# Page-range parsing errors come from the core and stay English.

extract-no-document = Open a PDF before extracting pages from it.
extract-no-pages = This document has no pages to extract.
extract-not-editable = This document cannot be extracted from as an editable PDF.
extract-document-changed = The document changed while the destination was being chosen. Try again.
# Keep "1-3,7" as typed: it is an example of the accepted page-range syntax.
extract-empty-selection = Type which pages to extract, for example 1-3,7.
extract-dialog-title = Extract pages
extract-pages-label = Pages to extract
# $count (Number) - pages in the document.
extract-range-hint =
    { $count ->
        [one] This document has { $count } page.
       *[other] This document has { $count } pages.
    }
extract-cancel = Cancel
extract-accept = Extract
# $count (Number) - pages extracted; $path (String) - destination path.
extract-summary =
    { $count ->
        [one] Extracted { $count } page to { $path }.
       *[other] Extracted { $count } pages to { $path }.
    }
extract-signed-note = The original document is signed, so the extracted PDF's signature no longer verifies.
extract-chooser-title = Save extracted PDF
extract-cancelled = Extraction cancelled.
extract-busy = Extracting pages...
extract-done = Pages extracted.
# $error (String) - technical detail, kept verbatim.
extract-failed = Could not extract pages: { $error }
