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

## Signing: identity picker, refusals and writing the signed PDF

sign-picker-title = Choose a signing identity
sign-picker-prompt = Sign this document with:
sign-picker-sign = Sign
sign-picker-none-chosen = Choose a signing identity.
sign-cancelled = Signing cancelled.
sign-chooser-title = Save signed PDF
sign-busy = Signing PDF...
sign-done = PDF signed and reopened.
sign-failed = Could not sign PDF: { $error }
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

## Write chains: shared by every write (destination chooser, replace guard,
## signature warning). File-system details arrive untranslated as { $error }.

write-no-save-backing = This document has no save backing.
write-pdf-filter = PDF files
write-accept = Save
write-not-local-file = The selected location is not a local file.
write-check-failed = Could not check whether { $path } already exists: { $error }
write-replace-title = Replace existing PDF?
write-replace-cancel = Cancel
write-replace = Replace
write-signature-title = Saving will break this document's signature
write-signature-detail =
    This document is signed. Saving rewrites the file, so the signature will no longer match what it covers.

    It is not removed: the saved file still carries the signature, and PDF readers will report it as invalid rather than missing.

    To keep a copy that still verifies, cancel and save to a different file.
write-signature-cancel = Cancel
write-signature-proceed = Save anyway

## Save: refusals and progress

save-no-document = Open a PDF before saving.
save-not-editable = This document cannot be saved as an editable PDF.
save-chooser-title = Save PDF
save-cancelled = Save cancelled.
save-busy = Saving PDF...
save-done = PDF saved and reopened.
save-failed = Could not save PDF: { $error }

## Preview refresh after an edit

preview-refreshing = Refreshing preview...
preview-failed = Could not refresh preview: { $error }

## Protect: password dialog, hints and progress

protect-no-document = Open a PDF before protecting it.
protect-not-protectable = This document cannot be protected.
protect-dialog-title = Protect with a password
protect-open-label = Password to open the document
protect-permissions-label = Permissions password
protect-permissions-tooltip = Governs what readers may do with the document once it is open, and lifts those restrictions for whoever holds it. It must not be the same as the open password.
protect-cancel = Cancel
protect-accept = Protect
protect-open-missing = Enter the password the document will ask for when it is opened.
protect-permissions-missing = Enter the permissions password.
protect-same-passwords = The two passwords must be different. If they were the same, everyone who can open the document could also change what it permits.
protect-chooser-title = Save protected PDF
protect-cancelled = Protection cancelled.
protect-busy = Protecting PDF...
protect-done = PDF protected, saved and reopened.
protect-failed = Could not protect PDF: { $error }

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

## Opening documents: chooser, progress, results and page navigation

open-chooser-title = Open PDF
open-accept = Open
open-not-local-file = The selected location is not a local file.
open-window-unavailable = The application window is unavailable.
open-blank-failed = Could not create a blank PDF.
open-busy = Opening PDF...
# $error (String) - technical detail from the renderer, kept verbatim.
open-failed = Could not open PDF: { $error }
open-no-pages = The PDF contains no pages.
# $page (Number) - one-based page number.
open-go-to-page = Go to page { $page }
# Document names shown on Organize's document card and used as the stem of
# exported and split file names, when the document has no file name of its own.
open-name-fallback = Opened PDF
open-name-sample = Sample document
open-name-untitled = Untitled document

## Opening documents: unsaved-changes prompt (before opening another document
## or closing the window)

open-unsaved-title = Unsaved changes
open-unsaved-detail-opening = The open document has changes that are not saved. Opening another document will discard them.
open-unsaved-detail-closing = The open document has changes that are not saved. Closing Vitela will discard them.
open-unsaved-cancel = Cancel
open-unsaved-discard = Discard
open-unsaved-save = Save
open-unsaved-kept = Kept the unsaved changes.

## Opening documents: password prompt for an encrypted PDF

open-password-title = Password required
open-password-cancel = Cancel
open-password-open = Open
open-password-busy = Opening password-protected PDF...
open-password-waiting = Waiting for the document password.
open-password-wrong = The password is incorrect. Try again.
open-password-cancelled = Password entry cancelled.

## Export images: refusals, dialog, folder and results
# Page-range parsing errors come from the core and stay English.

export-no-document = Open a PDF before exporting it.
export-no-pages = The PDF has no pages to export.
export-document-no-pages = This document has no pages to export.
export-dialog-title = Export pages as images
export-pages-label = Pages
export-all-pages = All pages
export-current-page = Current page
export-custom-pages = Pages
export-format-label = Format
export-resolution-label = Resolution (DPI)
export-cancel = Cancel
export-accept = Export
# $page (Number) - one-based page number; $dpi (Number) - chosen resolution.
export-page-too-large = Page { $page } is too large to render at { $dpi } DPI. Choose a lower resolution.
export-cancelled = Export cancelled.
export-chooser-title = Export images into folder
export-not-local-folder = The selected location is not a local folder.
# $count (Number) - pages being exported.
export-busy =
    { $count ->
        [one] Exporting { $count } page…
       *[other] Exporting { $count } pages…
    }
export-unfinished = The export did not finish.
# $count (Number) - pages written; $folder (String) - folder path.
export-summary =
    { $count ->
        [one] Exported { $count } page to { $folder }.
       *[other] Exported { $count } pages to { $folder }.
    }
# $error (String) - technical detail, kept verbatim.
export-prepare-failed = Could not prepare the document for export: { $error }
# $page (Number) - one-based page number; $error (String) - technical detail, kept verbatim.
export-page-failed = Page { $page } could not be exported: { $error }
# $name (String) - image file name; $error (String) - technical detail, kept verbatim.
export-write-failed = Could not write { $name }: { $error }

## Add watermark: refusals and dialog
# Page-range parsing errors come from the core and stay English.

watermark-no-document = Open a PDF before adding a watermark.
watermark-cancelled = Watermark cancelled.
watermark-dialog-title = Add watermark
watermark-kind-label = Stamp
watermark-kind-text = Text
watermark-kind-picture = Picture
watermark-text-label = Text
# Pre-filled in the text entry and stamped into the PDF as typed.
watermark-default-text = DRAFT
watermark-size-label = Size (pt)
watermark-choose-picture = Choose picture…
watermark-no-picture = No picture chosen
watermark-picture-label = Picture (PNG or JPEG)
watermark-scale-label = Size (% of page)
watermark-pages-label = Pages
watermark-all-pages = All pages
watermark-current-page = Current page
watermark-custom-pages = Pages
watermark-opacity-label = Opacity (%)
watermark-angle-label = Angle (degrees)
watermark-position-label = Position
watermark-over = Over the page content
watermark-under = Behind the page content
watermark-cancel = Cancel
watermark-accept = Add watermark
watermark-chooser-title = Choose watermark picture
watermark-chooser-accept = Choose
watermark-not-local-file = The selected location is not a local file.
# $path (String) - file path; $error (String) - technical detail, kept verbatim.
watermark-read-failed = Could not read { $path }: { $error }

## Add watermark: validation and results

watermark-no-pages = This document has no pages to stamp.
watermark-text-required = Type the text to stamp on the pages.
watermark-picture-required = Choose a PNG or JPEG picture to stamp on the pages.
# $page (Number) - one-based page number.
watermark-no-such-page = This document has no page { $page }.
# $character (String) - the first character the stamp font cannot show.
watermark-font-gap = The watermark font cannot show “{ $character }”. Use Latin letters, digits and punctuation.
watermark-invalid-picture = That file could not be read as a PNG or JPEG picture. Choose another one.
watermark-not-recordable = This watermark cannot be added to these pages.
# $count (Number) - pages stamped.
watermark-summary =
    { $count ->
        [one] Watermark added to { $count } page.
       *[other] Watermark added to { $count } pages.
    }

## Shared refusals: document permissions and editable-model availability

refusal-text-forbidden = This document does not permit copying or extracting its text.
refusal-text-unreadable = This document's permissions could not be read, so its text cannot be extracted.
refusal-annotation-forbidden = This document does not permit annotation changes.
refusal-annotation-model-unavailable = This document could not be prepared for annotation changes.
refusal-content-forbidden = This document does not permit content changes.
refusal-content-model-unavailable = This document could not be prepared for content changes.
refusal-assembly-forbidden = This document does not permit adding, removing or reordering its pages.
refusal-assembly-unreadable = This document's permissions could not be read, so its pages cannot be changed.
refusal-rewrite-needs-passwords = Changing this encrypted document's pages rewrites the whole file, which needs both its user and owner passwords. Reopen it with both to continue.
refusal-rewrite-unsupported-encryption = This document's encryption cannot yet be reproduced when the file is rewritten, so its pages cannot be changed.
refusal-page-no-longer-present = That page is no longer part of the document.

## Annotate: tool buttons and controls

annotate-tool-highlight = Highlight
annotate-tool-underline = Underline
annotate-tool-strikeout = Strikeout
annotate-tool-ink = Ink
annotate-tool-note = Note
annotate-tool-text-box = Text box
annotate-tool-shape = Shape
annotate-tool-stamp = Stamp
annotate-previous = Previous annotation
annotate-nudge = Nudge
annotate-grow = Grow
annotate-restyle = Restyle
annotate-edit-text = Edit text
annotate-delete = Delete
annotate-color-title = Choose annotation color

## Annotate: status messages and refusals

# $tool (String) - the localized tool name (annotate-tool-*).
annotate-tool-armed = { $tool } armed — drag on a page to place it.
annotate-tool-disarmed = { $tool } disarmed.
annotate-markup-applied = { $tool } applied to the selected text. Changes are pending save.
annotate-added = { $tool } added. Changes are pending save.
annotate-not-markup = { $tool } does not mark up text.
# $error (String) - technical detail from the image decoder, kept verbatim.
annotate-image-unusable = Could not use the image: { $error }
annotate-stamp-added = Image stamp added. Changes are pending save.
annotate-undone = Edit undone.
annotate-redone = Edit redone.
annotate-undone-pending = Edit undone. Changes are pending save.
annotate-redone-pending = Edit redone. Changes are pending save.
annotate-edited = Annotation edited. Changes are pending save.
annotate-deleted = Annotation deleted. Changes are pending save.
# $id (Number) - the annotation's internal number.
annotate-selected = Selected annotation { $id }.
annotate-moved = Annotation moved. Changes are pending save.
annotate-resized = Annotation resized. Changes are pending save.
annotate-color-stale = Color selection is no longer current.
annotate-restyled = Annotation restyled. Changes are pending save.
annotate-ink-needs-drag = Drag to draw an ink annotation.
annotate-note-needs-text = Type the note's text before adding it.
annotate-no-document = Open a PDF before editing annotations.
annotate-selection-gone = The selected annotation no longer exists.
annotate-select-first = Select an annotation first.
annotate-drag-unsupported = That annotation cannot be changed this way.

## Annotate: note and text box dialogs

annotate-note-canceled = Note placement canceled.
annotate-note-no-dialog = Could not open the note dialog.
# $page (Number) - one-based page number.
annotate-note-title = Add note — page { $page }
annotate-note-text = Note text
annotate-dialog-cancel = Cancel
annotate-dialog-add = Add
annotate-dialog-save = Save
annotate-dialog-text = Text
annotate-note-added = Text note added. Changes are pending save.
annotate-text-box-add-canceled = Text box placement canceled.
annotate-text-box-edit-canceled = Text box edit canceled.
annotate-text-box-needs-text = Type the text box's text before adding it.
annotate-text-box-too-small = That text box is too small.
annotate-text-box-select-first = Select a text box first.
annotate-text-box-document-changed = The document changed while the dialog was open; nothing was added.
annotate-text-box-no-dialog = Could not open the text box dialog.
annotate-text-box-unchanged = Text unchanged.
# $character (String) - the first character the text box font cannot show.
annotate-text-box-encoding-gap = The character '{ $character }' can't be used in a text box: it is drawn in Helvetica, which only covers Western European text. Remove it and try again.
# $page (Number) - one-based page number.
annotate-text-box-add-title = Add text box — page { $page }
annotate-text-box-edit-title = Edit text box
annotate-text-box-added = Text box added. Changes are pending save.
annotate-text-box-edited = Text box edited. Changes are pending save.

## Edit PDF: content edit results and refusals

edit-no-window = The application window is unavailable.
edit-text-deleted = Text deleted.
edit-text-moved-unsaved = This text was moved and not yet saved — save and reopen before retyping it.
edit-text-edit-unavailable = This text is part of an unsaved edit that is no longer available — save and reopen before editing it again.
edit-text-updated = Text updated.
edit-text-inserted = Text inserted.
edit-insertion-not-deletable = This text hasn't been saved yet — undo the insertion to remove it.
edit-cancelled = Edit cancelled.
edit-text-retyped-unsaved = This text has an unsaved edit — save the document before moving it.
edit-no-text-there = There is no text there to edit.
edit-open-to-edit = Open a PDF to edit its content.
edit-open-to-copy = Open a PDF to copy its content.
edit-text-moved = Text moved.
edit-image-moved = Image moved.
edit-image-resized = Image resized.
edit-image-deleted = Image deleted.
edit-replace-image-title = Replace image
edit-replace-image-accept = Replace
edit-not-local-file = The selected location is not a local file.
# $path (String) - file path; $error (String) - technical detail, kept verbatim.
edit-read-failed = Could not read { $path }: { $error }
edit-image-replaced = Image replaced.
edit-image-files = Image files
edit-insert-image-title = Insert image
edit-insert-image-accept = Insert
# $error (String) - technical detail from the image decoder, kept verbatim.
edit-image-unusable = Could not use the image: { $error }
edit-image-inserted = Image inserted.
edit-shape-moved = Shape moved.
edit-shape-resized = Shape resized.
edit-shape-deleted = Shape deleted.

## Edit PDF: multi-select and content clipboard

# $count (Number) - selected items.
edit-multi-selected =
    { $count ->
        [one] { $count } item selected — drag to move, Delete removes, Esc clears.
       *[other] { $count } items selected — drag to move, Delete removes, Esc clears.
    }
edit-multi-nothing = Nothing there to select.
edit-multi-moved = Selection moved.
edit-multi-deleted = Selection deleted.
edit-multi-cleared = Selection cleared.
# $count (Number) - items placed on the content clipboard.
edit-clipboard-copied =
    { $count ->
        [one] Copied { $count } item.
       *[other] Copied { $count } items.
    }
edit-clipboard-cut =
    { $count ->
        [one] Cut { $count } item.
       *[other] Cut { $count } items.
    }
edit-clipboard-paste-off-page = Content can only be pasted onto a page of the document.
edit-clipboard-pasted = Pasted.

## Forms: toolbar, field kinds and fill panel

forms-kind-text = Text field
forms-kind-checkbox = Checkbox
forms-kind-radio-group = Radio group
forms-kind-dropdown = Dropdown
forms-edit-mode = Edit forms
forms-color = Color
forms-delete-field = Delete field
forms-fill-placeholder = Open a PDF with form fields to fill them in, or place one in Edit forms mode.
forms-heading-fields = Form fields
forms-heading-style = Field style
forms-heading-fill = Fill fields
forms-none-choice = (none)
forms-unsupported-field = Unsupported field
forms-field-updated = Form field updated.

## Forms: edit mode and field results

forms-value-set = Field value set. Changes are pending save.
forms-renamed = Field renamed. Changes are pending save.
forms-selection-gone = The selected form field no longer exists.
forms-mode-armed = Edit forms armed — pick a field type to place, or click an existing field to select it.
forms-mode-disarmed = Edit forms disarmed.
# $kind (String) - the localized field kind (forms-kind-*).
forms-kind-armed = { $kind } armed — drag on the page to place it.
forms-mode-armed-select = Edit forms armed — click an existing field to select it.
forms-no-document = Open a PDF before editing form fields.
forms-deleted = Field deleted. Save to keep the change, or undo to bring it back.
forms-added = { $kind } added. Changes are pending save.
forms-moved = Field moved. Changes are pending save.
forms-resized = Field resized. Changes are pending save.
forms-restyled = Field restyled. Changes are pending save.
forms-color-title = Choose field color
forms-color-stale = Color selection is no longer current.

## Viewer: dropping and pasting files

drop-not-local = Only local PDF and image files can be dropped.
paste-open-first = Open a PDF before pasting an image.
paste-no-image = Clipboard does not contain a bitmap image.
drop-page-not-ready = The page is not ready for image placement.
drop-loading = Loading dropped file...
drop-unsupported = Dropped file is neither a PDF nor a supported image.
# $error (String) - technical detail from the file system, kept verbatim.
drop-read-failed = Could not read dropped file: { $error }
drop-image-on-page = Drop an image directly onto an open PDF page.

## Viewer: printing

print-open-first = Open a PDF before printing.
print-preparing = Preparing to print…
# $error (String) - technical detail, kept verbatim.
print-prepare-failed = Could not prepare the document for printing: { $error }
print-prepare-stopped = Could not prepare the document for printing.
print-no-pages = The PDF has no pages to print.
print-failed = Printing failed.
# $error (String) - technical detail from GTK, kept verbatim.
print-error = Could not print: { $error }

## Viewer: page rendering status

# $first, $last, $total (Number) - one-based visible page range and page count.
render-showing-pages = Showing pages { $first }-{ $last } of { $total }.
# $page (Number) - one-based page number.
render-page-too-large = Page { $page } cannot be rendered safely at this size.
# $page, $total (Number) - one-based page and page count; $error (String) - renderer detail, kept verbatim.
render-page-failed = Could not render page { $page } of { $total }: { $error }
render-sharpen-failed = Could not sharpen page { $page } of { $total }: { $error }
# $page (Number) - one-based page; $returned, $requested (Number) - tile counts.
render-tile-mismatch = Page { $page } returned { $returned } tiles for { $requested } requested.

## Viewer: text selection

selection-copy-empty = Select some text before copying.
# $count (Number) - characters copied.
selection-copied =
    { $count ->
        [one] Copied { $count } character to the clipboard.
       *[other] Copied { $count } characters to the clipboard.
    }
