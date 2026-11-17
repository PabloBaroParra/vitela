# English strings for the Windows (WinUI) shell. This file is the reference:
# es.ftl must define exactly the same message ids (a facade test enforces it).
#
# Keys are shared with the GTK shell (apps/linux-gtk/locales): where Windows
# shows the same English text as Linux it uses Linux's key and Linux's Spanish
# verbatim, so a translation reads the same on both platforms. Sections headed
# "Windows only" hold strings Linux does not have.

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
# $date (String) - the short date in the user's regional format, already formatted (e.g. 11/7/2026).
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
home-shortcut-command-palette = Command palette

## Home: version footer

# $version (String) - the release version, or home-version-dev.
home-version = Vitela { $version }
home-version-dev = dev

## Home: Windows only

home-return-to-document = Return to document
# Accessible name of the list of recent-document cards.
home-recent-files = Recent files

## Mode bar (top of the window)

mode-bar-sections = Vitela sections
mode-home = Home
mode-edit = Edit
mode-annotate = Annotate
mode-organize = Organize
mode-sign = Sign
mode-protect = Protect

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

## Mode bar: Windows only (the rail also lists Recent and My files)

mode-recent = Recent
mode-files = My files
mode-edit-pdf = Edit PDF
mode-organize-pages = Organize pages

## Editor: Windows only toolbar commands, groups and page navigation

editor-share = Share
editor-share-tooltip = Send a copy with Nearby Sharing, Phone Link or another app
editor-undo-tooltip = Undo (Ctrl+Z)
editor-redo-tooltip = Redo (Ctrl+Y)
editor-pages-tooltip = Show or hide the page list
editor-tools-tooltip = Show or hide tools
editor-group-page-navigation = Page navigation
editor-group-more-tools = More document tools
editor-group-document-title = Document title
editor-first-page = First page
editor-first-page-tooltip = Go to the first page of the document (Ctrl+Home)
editor-previous-page = Previous page
editor-previous-page-tooltip = Go to the page before the current viewport page (Ctrl+Up)
editor-next-page = Next page
editor-next-page-tooltip = Go to the page after the current viewport page (Ctrl+Down)
editor-last-page = Last page
editor-last-page-tooltip = Go to the last page of the document (Ctrl+End)
editor-go-to-page = Go to page
editor-go-to-page-tooltip = Jump to a page by number (Ctrl+G)
editor-protect = Protect
editor-compress = Compress
editor-extract-pages = Extract pages
editor-split-pages = Split PDF
editor-split-pages-tooltip = Cut the document into several PDFs
editor-organize = Organize
editor-organize-tooltip = Move, rotate or delete pages
# The button that runs the search typed in the Find flyout.
editor-find-submit = Find
editor-no-document = No document open
editor-empty-state = Drop a PDF here to open it, or try the built-in sample.

## Pages sidebar and side-column dividers: Windows only

# $page (Number) - one-based page number.
editor-page-item = Page { $page }
editor-resize-pages = Resize page list
editor-resize-pages-tooltip = Drag to resize or fold the page list, or use Left and Right arrows when focused
editor-resize-tools = Resize tools panel
editor-resize-tools-tooltip = Drag to resize or fold the tools panel, or use Left and Right arrows when focused
# $total (Number) - pages in the document.
editor-go-to-page-number = Page number (1–{ $total })
editor-go-to-page-go = Go
editor-go-to-page-cancel = Cancel

## Protect: Windows only

# Status line when the rail's Protect is chosen on a document that forbids it.
protect-forbidden = This document does not permit changes to its protection.

## Organize: shared with the editor toolbar's Extract pages

organize-extract-tooltip = Save chosen pages as a new PDF

## Search: Windows only

# $page (Number) - one-based page number; $text (String) - the matched text, verbatim.
search-result = Page { $page }: { $text }
# $error (String) - technical detail, kept verbatim.
search-navigate-failed = Could not navigate to the match: { $error }

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

## Tools panel: page headings

tools-comments = Comments
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

## Tools panel: Windows only tabs (the GTK shell had them until its panel was
## redesigned; same keys and Spanish as its #542)

tools-pages = Tools pages
tools-tab-edit = Edit
tools-tab-annotate = Annotate
tools-tab-comments = Comments
tools-tab-fill-sign = Fill & Sign

## Document properties: Windows only

# $count (Number) - pages in the document.
metadata-pages-value = Pages: { $count }
metadata-pages-none = Pages: —
metadata-forbidden = This document does not permit metadata changes.

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
annotate-delete = Delete
annotate-color-title = Choose annotation color

## Annotate: status messages and refusals

annotate-stamp-added = Image stamp added. Changes are pending save.
annotate-undone-pending = Edit undone. Changes are pending save.
annotate-redone-pending = Edit redone. Changes are pending save.
annotate-edited = Annotation edited. Changes are pending save.
annotate-moved = Annotation moved. Changes are pending save.
annotate-resized = Annotation resized. Changes are pending save.
annotate-restyled = Annotation restyled. Changes are pending save.

## Annotate: note and text box dialogs

annotate-note-canceled = Note placement canceled.
# $page (Number) - one-based page number.
annotate-note-title = Add note — page { $page }
annotate-note-text = Note text
annotate-dialog-cancel = Cancel
annotate-dialog-add = Add
annotate-dialog-save = Save
annotate-dialog-text = Text
annotate-text-box-add-canceled = Text box placement canceled.
annotate-text-box-edit-canceled = Text box edit canceled.
# $page (Number) - one-based page number.
annotate-text-box-add-title = Add text box — page { $page }
annotate-text-box-added = Text box added. Changes are pending save.
annotate-text-box-edited = Text box edited. Changes are pending save.

## Annotate: Windows only (toolbar commands, numeric geometry dialogs, note reading)

annotate-pointer = Pointer
annotate-text-box-tooltip = Click or drag on a page to add a text box
annotate-previous-tooltip = Select the previous annotation, including ones underneath others
annotate-next = Next annotation
annotate-next-tooltip = Select the next annotation, including ones underneath others
annotate-edit-text-box = Edit text box
annotate-edit-text-box-tooltip = Retype the selected text box
annotate-read-note = Read note
annotate-read-note-tooltip = Read the selected note without changing the document
annotate-nudge-tooltip = Shift the selected annotation
annotate-move = Move annotation
annotate-move-tooltip = Set the selected annotation's X and Y in PDF points
annotate-grow-tooltip = Enlarge the selected annotation
annotate-resize = Resize annotation
annotate-resize-tooltip = Set the selected annotation's width and height in PDF points
annotate-pointer-mode = Pointer mode.
annotate-text-box-armed = Text box armed. Click or drag on a page to place it.
# $tool (String) - the localized tool name (annotate-tool-*).
annotate-armed = { $tool } armed. Drag on a page to place it.
annotate-ink-needs-drag-not-tap = Ink needs a drag, not a tap.
annotate-image-unsupported = The image is not a supported PNG or JPEG.
annotate-stamp-canceled = Stamp placement canceled.
annotate-stamp-unreadable = The selected stamp image could not be read.
# $id (Number) - the annotation's internal number; $page (Number) - one-based page number.
annotate-selected-on-page = Selected annotation { $id } on page { $page }.
# $page (Number) - one-based page number.
annotate-move-title = Move annotation — page { $page }
annotate-move-explanation = Position the annotation's bottom-left bounds in unrotated PDF space. X increases rightward; Y increases upward. Size stays unchanged.
annotate-position-unchanged = Annotation position unchanged.
# $page (Number) - one-based page number.
annotate-resize-title = Resize annotation — page { $page }
annotate-resize-explanation = The annotation stays at its current PDF-space origin. Width and height are independent; stamps stretch to fit.
annotate-dimensions-unchanged = Annotation dimensions unchanged.
# $page (Number) - one-based page number.
annotate-note-read-title = Note — page { $page }
annotate-dialog-close = Close
# $page (Number) - one-based page number.
annotate-text-box-edit-page-title = Edit text box — page { $page }

## Color dialog: Windows only (shared by annotations and form fields)

editor-color-choose = Choose
editor-color-cancel = Cancel

## Numeric geometry dialogs: Windows only (annotations, images and text)

geometry-x = X (pt)
geometry-y = Y (pt)
geometry-width = Width (pt)
geometry-height = Height (pt)
geometry-move = Move
geometry-resize = Resize
geometry-cancel = Cancel

## Shared refusals: document permissions and editable-model availability

refusal-content-forbidden = This document does not permit content changes.

## Edit PDF: panel controls, tooltips and gesture hints

edit-heading = Edit PDF
edit-card-text = Text
edit-card-images = Images
edit-card-shapes = Shapes
edit-card-pages = Pages
edit-mode = Edit content
edit-insert-text = Insert text
edit-insert-text-tooltip = Click the page to place a new text box
edit-delete-text = Delete text
edit-insert-image = Insert image
edit-insert-image-tooltip = Click the page to insert a picture
edit-replace-image = Replace image
edit-replace-image-tooltip = Swap the selected image for a file on disk
edit-delete-image = Delete image
edit-delete-image-tooltip = Remove the selected image from the page
edit-delete-shape = Delete shape
edit-watermark = Add watermark
edit-watermark-hint = A watermark is painted into the pages themselves and is one undo step.
edit-no-document = Open a PDF to edit the text and images on its pages.
edit-text-selected = Text selected — drag to move it, press Delete to remove it, double-click or F2 to retype it.
edit-no-image-selected = Click an image on the page to select it.
edit-image-selected = Image selected — replace or delete it.

## Edit PDF: mode activation messages

edit-insert-text-armed = Insert text armed — click the page to place a new text box.
edit-insert-image-armed = Insert image armed — click the page to insert a picture.

## Edit PDF: content edit results and refusals

edit-cancelled = Edit cancelled.

## Edit PDF: multi-select and content clipboard

edit-multi-nothing = Nothing there to select.
edit-multi-cleared = Selection cleared.

## Edit PDF: Windows only (panel wording, numeric geometry dialogs, statuses
## that end in "Save to keep the change.")

edit-description-text-images = Change the text and images already on the page.
edit-mode-run-tooltip = Turn on content editing, then click a text run on the page
edit-delete-text-run-tooltip = Remove the text run being edited from the page (not secure redaction)
edit-watermark-picture-tooltip = Stamp the same text or picture across the pages you choose
edit-delete-shape-line-tooltip = Remove the selected line or box from the page
edit-resize-image = Resize image
edit-resize-image-tooltip = Resize a content image on the visible page
edit-move-image = Move image
edit-move-image-tooltip = Move a content image on the visible page
edit-move-text = Move text
edit-move-text-tooltip = Move a text run on the visible page
edit-no-pages = This PDF has no pages to edit.
edit-busy = Please wait while the document is being updated.
edit-return-to-document = Return to the document to edit its text and images.
edit-text-run-hint = Click a text run to retype it in place.
edit-text-run-selected = Text run selected — delete it, or retype it in place.
edit-no-text-selected-retype = Click text to select it; double-click to retype it in place.
edit-image-pending-edit = This image has an unsaved edit — move, resize or delete it. Save before replacing it.
edit-image-page-changed = The page changed — select the image again.
edit-no-shape-selected-line = Click a line or the border of a box to select it.
edit-shape-selected-resize = Shape selected — drag it, drag a corner to resize, or press Delete.
edit-shape-page-changed = The page changed — select the shape again.
edit-content-editing-off = Content editing off.
edit-content-editing-armed = Content editing armed — click text to select it, double-click to retype it.
edit-content-editing-armed-run = Content editing armed — click a text run to retype it.
edit-no-editable-text = No editable text there — click a word to retype it.
edit-retype-hint = Type to edit the text — Escape puts it back.
edit-retype-substituted = Type replacement text — it will use Helvetica. Escape puts it back.
edit-font-substitution-title = Replace this text's font?
edit-font-substitution-body = This PDF uses a composite font that cannot be retyped safely. Editing will replace this text with Helvetica while keeping its position and size. The appearance may change slightly.
edit-font-substitution-accept = Replace font and edit
edit-font-substitution-cancel = Cancel
edit-insert-text-typing = Type the new text — Enter inserts it, Escape cancels.
edit-insertion-cancelled = Insertion cancelled.
edit-text-changed-before-delete = The text changed. Click the run again before deleting it.
edit-image-unreadable = The selected image could not be read.
edit-text-updated-save = Text updated. Save to keep the change.
edit-text-inserted-save = Text inserted. Save to keep the change.
edit-text-moved-save = Text moved. Save to keep the change.
edit-text-deleted-save = Text deleted. Save to keep the change.
edit-image-inserted-save = Image inserted. Save to keep the change.
edit-image-replaced-save = Image replaced. Save to keep the change.
edit-image-moved-save = Image moved. Save to keep the change.
edit-image-resized-save = Image resized. Save to keep the change.
edit-image-deleted-save = Image deleted. Save to keep the change.
edit-shape-moved-save = Shape moved. Save to keep the change.
edit-shape-resized-save = Shape resized. Save to keep the change.
edit-shape-deleted-save = Shape deleted. Save to keep the change.
edit-selection-moved-save = Selection moved. Save to keep the change.
edit-selection-deleted-save = Selection deleted. Save to keep the change.
# $count (Number) - selected items.
edit-items-selected =
    { $count ->
        [one] { $count } item selected — drag inside the dashed box to move them, press Delete to remove them, SHIFT+click to add or remove one.
       *[other] { $count } items selected — drag inside the dashed box to move them, press Delete to remove them, SHIFT+click to add or remove one.
    }
edit-clipboard-cut-hint = Cut. Point where it should go and press Ctrl+V.
# $count (Number) - items placed on the content clipboard.
edit-clipboard-copied-hint =
    { $count ->
        [one] Copied { $count } item. Point where it should go and press Ctrl+V.
       *[other] Copied { $count } items. Point where it should go and press Ctrl+V.
    }
edit-clipboard-pasted-hint = Pasted. Drag it into place, or save to keep the change.
# $count (Number) - pasted items, now selected.
edit-clipboard-pasted-count =
    { $count ->
        [one] Pasted { $count } item — drag inside the dashed box to place them.
       *[other] Pasted { $count } items — drag inside the dashed box to place them.
    }
edit-no-content-images = The visible page has no editable content images.
edit-image-choice = Image
# $index (Number) - one-based image number; $x, $y (String) - coordinates in points, already formatted.
# Spanish separates the two with a semicolon: its decimal comma would make "X 1,5, Y 3" ambiguous.
edit-image-choice-item = Image { $index }: X { $x }, Y { $y } pt
# $page (Number) - one-based page number.
edit-image-move-title = Move image — page { $page }
# $page (Number) - one-based page number.
edit-image-resize-title = Resize image — page { $page }
edit-image-move-explanation = Coordinates use PDF space: X increases rightward and Y upward. The image keeps its size. Save before making another geometry edit to the same image.
edit-image-resize-explanation = The image stays at its current origin. Width and height are independent. Save before making another geometry edit to the same image.
edit-image-position-unchanged = Image position unchanged.
edit-image-dimensions-unchanged = Image dimensions unchanged.
edit-no-text-runs = The visible page has no editable text runs.
edit-text-run-choice = Text run
# $page (Number) - one-based page number.
edit-text-move-title = Move text — page { $page }
edit-text-move-explanation = Coordinates use PDF space: X increases rightward and Y upward. The text keeps its font and size. Repeated moves share one Undo step; moving newly inserted text updates its insertion. Save and reopen first if existing text has been retyped.
edit-text-position-unchanged = Text position unchanged.

## Comments: Windows only (the Comments tools page lists the document's comments)

comments-empty = No comments in this document.
# $page (Number) - one-based page number; heads that page's comments.
comments-page = Page { $page }
# $page (Number) - one-based page number.
comments-read-title = Comment — page { $page }
comments-close = Close

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
forms-unsupported-field = Unsupported field

## Forms: edit mode and field results

forms-mode-armed = Edit forms armed — pick a field type to place, or click an existing field to select it.
forms-mode-disarmed = Edit forms disarmed.
forms-deleted = Field deleted. Save to keep the change, or undo to bring it back.
forms-moved = Field moved. Changes are pending save.
forms-resized = Field resized. Changes are pending save.
forms-color-title = Choose field color

## Forms: Windows only (style controls, field names, placement hints and
## statuses that end in "Save to keep the change.")

forms-style-font = Font
forms-style-size = Size (pt)
# "(Delete)" names the keyboard key that also deletes the field: write it as
# the key is labelled on keyboards in the target language (Spanish: "Supr").
forms-delete-field-tooltip = Delete the selected field (Delete)
forms-fill-forbidden = This document does not permit filling in its form.
# $name (String) - the field's name, verbatim.
forms-field-name = Field name: { $name }
forms-field-name-tooltip = Type to rename. Enter accepts; Escape cancels.
forms-place-text-hint = Click or drag on a page to place a text field.
forms-place-checkbox-hint = Click or drag on a page to place a checkbox.
forms-place-radio-group-hint = Click or drag on a page to place a radio group.
forms-place-dropdown-hint = Click or drag on a page to place a dropdown.
forms-placed-text = Text field placed. Save to keep the change.
forms-placed-checkbox = Checkbox placed. Save to keep the change.
forms-placed-radio-group = Radio group placed. Save to keep the change.
forms-placed-dropdown = Dropdown placed. Save to keep the change.
forms-renamed-save = Field renamed. Save to keep the change.
forms-moved-save = Field moved. Save to keep the change.
forms-resized-save = Field resized. Save to keep the change.
forms-font-size-changed-save = Field font size changed. Save to keep the change.
forms-font-changed-save = Field font changed. Save to keep the change.
forms-color-changed-save = Field text color changed. Save to keep the change.
forms-filled-save = Field filled in. Save to keep the change.

## Add watermark: refusals and dialog
# Page-range parsing errors come from the facade and stay English.

watermark-cancelled = Watermark cancelled.
watermark-dialog-title = Add watermark
watermark-kind-text = Text
watermark-kind-picture = Picture
watermark-text-label = Text
# Pre-filled in the text entry and stamped into the PDF as typed.
watermark-default-text = DRAFT
watermark-size-label = Size (pt)
watermark-choose-picture = Choose picture…
watermark-scale-label = Size (% of page)
watermark-pages-label = Pages
watermark-all-pages = All pages
watermark-current-page = Current page
watermark-custom-pages = Pages
watermark-opacity-label = Opacity (%)
watermark-over = Over the page content
watermark-under = Behind the page content
watermark-cancel = Cancel

## Add watermark: Windows only (dialog wording and the result that ends in
## "Save to keep the change.")

watermark-kind-header = Watermark
watermark-no-picture-hint = No picture chosen. PNG or JPEG.
# $size (Number) - the picture's size in kB, at least 1.
watermark-picture-chosen = Picture chosen ({ $size } kB).
# Accessible name of the custom page-range entry.
watermark-page-range = Page range
watermark-angle-degrees-label = Angle (°)
watermark-placement-label = Placement
watermark-add = Add
# $count (Number) - pages stamped.
watermark-summary-save =
    { $count ->
        [one] Watermark added to { $count } page. Save to keep the change.
       *[other] Watermark added to { $count } pages. Save to keep the change.
    }

## Organize: header, view switch and hints

organize-heading = Organize pages
organize-import-progress-name = PDF import progress
organize-undo = Undo
organize-redo = Redo
organize-add-pdfs = Add PDFs
organize-extract = Extract
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
organize-blank-pages = Blank pages
organize-blank-page = Blank page
organize-imported-pdf = Imported PDF
organize-page-rotate-left = Rotate page left
organize-page-rotate-right = Rotate page right
organize-page-delete = Delete page

## Organize: adding PDFs (progress, passwords and results)

organize-import-cancelled = PDF import cancelled.
organize-import-checking = Checking selected PDFs...
# $completed, $total (Number) - PDFs prepared so far, out of the selection.
organize-import-progress = { $completed } of { $total } PDFs
# $name (String) - file name; $error (String) - technical detail, kept verbatim.
organize-import-failed = Could not import { $name }: { $error }
organize-import-password-title = Password required
organize-import-password-prompt = Enter the password for { $name }.
organize-import-password-wrong = The password is incorrect. Try again.
organize-import-password-cancel = Cancel import
organize-import-unlock = Unlock
organize-import-password-waiting = Waiting for the password for { $name }.
organize-import-warning-title = Some document-level information will stay behind
organize-import-warning-cancel = Cancel
organize-import-anyway = Import anyway

## Organize: Windows only (toolbar, page-card menu and statuses that end in
## "Changes are pending save.")

organize-add-pdfs-tooltip = Add every page of other PDFs to the end
organize-cancel-import-button = Cancel import
organize-add-blank-page = Add blank A4 page
organize-add-landscape-page = Add landscape A4 page
organize-rotate-all-left = Rotate all left
organize-rotate-all-left-tooltip = Turn every page a quarter turn counter-clockwise
organize-rotate-all-right = Rotate all right
organize-rotate-all-right-tooltip = Turn every page a quarter turn clockwise
organize-return = Return to document
organize-entered-hint = Drag a page to move it. Changes are one undo step each.
# $page (Number) - one-based page position; the page card's accessible name.
organize-page-name = Page { $page }
organize-page-actions = Page actions
organize-page-move-earlier = Move page earlier
organize-page-move-later = Move page later
organize-page-insert-portrait = Insert portrait A4 before
organize-page-insert-landscape = Insert landscape A4 before
# $name (String) - the document card's title.
organize-block-delete-named = Delete: { $name }
organize-blank-page-added-pending = Blank page added. Changes are pending save.
organize-page-moved-pending = Page moved. Changes are pending save.
organize-page-deleted-pending = Page deleted. Changes are pending save.
organize-page-rotated-pending = Page rotated. Changes are pending save.
organize-document-moved-pending = Document moved. Changes are pending save.
organize-document-deleted-pending = Document deleted. Changes are pending save.
organize-document-rotated-pending = Document rotated. Changes are pending save.
organize-import-pick-failed = Could not select PDFs to import.
organize-import-cancelling = Cancelling after the current PDF...
# $name (String) - file name.
organize-import-unreadable = Could not import { $name }: the file could not be read.
organize-import-adding = Adding selected PDFs...
organize-import-selected-failed = Could not import the selected PDFs.
organize-import-password-placeholder = Password
# $name (String) - file name; accessible name of the password entry.
organize-import-password-name = Password for { $name }
organize-import-warning-detail = No pages have been added. Import anyway to add the whole batch as one undo step.
# $pages (Number) - pages imported; $files (Number) - PDFs they came from.
organize-imported-summary =
    Imported { $pages ->
        [one] { $pages } page
       *[other] { $pages } pages
    } from { $files ->
        [one] { $files } PDF
       *[other] { $files } PDFs
    }. Changes are pending save.

## Write chains: shared by every write (destination chooser)

write-accept = Save

## Protect: password dialog, hints and progress

protect-dialog-title = Protect with a password
protect-open-label = Password to open the document
protect-permissions-label = Permissions password
protect-permissions-tooltip = Governs what readers may do with the document once it is open, and lifts those restrictions for whoever holds it. It must not be the same as the open password.
protect-cancel = Cancel
protect-accept = Protect
protect-open-missing = Enter the password the document will ask for when it is opened.
protect-permissions-missing = Enter the permissions password.
protect-same-passwords = The two passwords must be different. If they were the same, everyone who can open the document could also change what it permits.
protect-cancelled = Protection cancelled.

## Compress: dialog, presets and sizes

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
compress-cancelled = Compression cancelled.
compress-busy = Compressing PDF...

## Split: dialog, folder and results
# Cut-list parsing errors come from the facade and stay English.

split-dialog-title = Split PDF
split-cuts-label = Split after page
# $total (Number) - pages in the document, always two or more.
split-range-hint = This document has { $total } pages. Each cut starts a new file.
split-cancel = Cancel
split-accept = Split
# $count (Number) - part files that already exist in the folder.
split-replace-title =
    { $count ->
        [one] Replace { $count } existing PDF?
       *[other] Replace { $count } existing PDFs?
    }
# $total (Number) - files the split writes; $count (Number) - those that already exist.
split-replace-detail = Splitting into this folder writes { $total } files named after the document, and { $count } of them already exist there.
split-replace = Replace
# $count (Number) - files written, always two or more; $folder (String) - folder path.
split-summary = Split into { $count } PDFs in { $folder }.
split-signed-note = The original document is signed, so the signature in each part no longer verifies.
split-cancelled = Split cancelled.

## Extract: dialog and results
# Page-range parsing errors come from the facade and stay English.

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

## Export images: dialog, folder and results
# Page-range parsing errors come from the facade and stay English.

export-dialog-title = Export pages as images
export-pages-label = Pages
export-all-pages = All pages
export-current-page = Current page
export-custom-pages = Pages
export-format-label = Format
export-resolution-label = Resolution (DPI)
export-cancel = Cancel
export-accept = Export
export-cancelled = Export cancelled.
# $error (String) - technical detail, kept verbatim.
export-prepare-failed = Could not prepare the document for export: { $error }

## Output tools: Windows only (cancelled pickers, progress and partial writes)

protect-saved-reopened = Protected PDF saved and reopened.
compress-nothing-written = Compression cancelled. No file was written.
compress-no-source-size = No source file size is available for this new document.
split-nothing-written = Split cancelled. No file was written.
# $part, $total (Number) - one-based part being written and parts in all.
split-writing-part = Writing part { $part } of { $total }...
# $written (Number) - parts already written; $name (String) - part file name;
# $error (String) - technical detail, kept verbatim.
split-stopped = Split stopped after { $written } written: { $name } could not be written: { $error }
extract-dialog-cancelled = Extract cancelled.
extract-nothing-written = Extract cancelled. No file was written.
# $count (Number) - pages being extracted.
extract-busy-count =
    { $count ->
        [one] Extracting { $count } page...
       *[other] Extracting { $count } pages...
    }
export-nothing-written = Export cancelled. No file was written.
export-preparing = Preparing to export...
# Accessible name of the custom page-range entry.
export-page-range = Page range
# $page (Number) - one-based page; $index, $total (Number) - file being written and files in all.
export-busy-page = Exporting page { $page } ({ $index } of { $total })...
# $page (Number) - one-based page; $error (String) - technical detail, kept verbatim;
# $written (Number) - images already written.
export-page-failed-written = Page { $page } could not be exported: { $error } { $written } written.
# $written (Number) - images already written; $error (String) - technical detail, kept verbatim.
export-stopped = Export stopped after { $written } written: { $error }
# $count (Number) - pages written; $format (String) - PNG or JPEG; $folder (String) - folder path.
export-summary-format =
    { $count ->
        [one] Exported { $count } page as { $format } to { $folder }.
       *[other] Exported { $count } pages as { $format } to { $folder }.
    }

## Signing: certificate section, unlock prompts and identity picker
# Adapter and facade error details arrive untranslated.

sign-heading = Signing
sign-choose-pfx = Choose signing certificate (.pfx)…
sign-use-token = Use card or token…
sign-use-nss = Use a certificate from this computer…
sign-signed-indicator = ✓ This document is digitally signed.
sign-cancel = Cancel
sign-unlock = Unlock
sign-pfx-password-title = Certificate password required
sign-pfx-reading = Reading certificate file...
sign-pfx-waiting = Waiting for the certificate password.
sign-pfx-wrong-password = The password is incorrect, or the file is not a valid PKCS#12 certificate.
sign-certificate-cancelled = Certificate selection cancelled.
sign-picker-title = Choose a signing identity
sign-picker-prompt = Sign this document with:
sign-picker-sign = Sign
sign-picker-none-chosen = Choose a signing identity.
sign-cancelled = Signing cancelled.

## Drawn signature: entry, pad, saved-signature offer and placement

signature-draw = Draw signature…
signature-pad-title = Draw your signature
signature-clear = Clear
signature-cancel = Cancel
signature-use = Use
signature-offer-title = Your signature
signature-offer-preview = Your saved signature
signature-offer-unshowable = This saved signature can no longer be shown.
signature-draw-new = Draw new
signature-delete = Delete
signature-unrenderable = The signature could not be turned into an image.
signature-not-remembered = Your signature could not be remembered. Click a page to place it.
signature-armed = Click a page to place your signature.
signature-disarmed = Signature disarmed.

## Signing and drawn signature: Windows only

sign-choose-pfx-tooltip = Choose a PKCS#12 signing certificate (.pfx or .p12).
sign-no-identities = This certificate source has no usable signing identities.
sign-busy-document = Signing document...
sign-saved-reopened = Signed PDF saved and reopened.
# Accessible name of the PIN entry.
sign-token-pin-name = Token PIN
sign-token-pin-prompt = Enter the token PIN, or leave it empty to use the token's own authentication prompt.
sign-token-title = Card or token authentication
sign-continue = Continue
# Accessible name of the password entry.
sign-pfx-password-name = Certificate password
# Accessible name of the list of identities.
sign-identity-name = Signing identity
signature-heading = Signature
signature-remember-pc = Remember on this PC
# Accessible name of the Draw signature… button.
signature-draw-name = Draw signature
signature-draw-tooltip-short = Draw a signature with the mouse, then click a page to place it.
signature-not-drawn = The signature could not be drawn.
signature-forgotten-pc = Your saved signature was deleted from this PC.

## Viewer: printing

print-open-first = Open a PDF before printing.
# $error (String) - technical detail, kept verbatim.
print-prepare-failed = Could not prepare the document for printing: { $error }
print-no-pages = The PDF has no pages to print.
print-failed = Printing failed.
# $error (String) - technical detail from Windows, kept verbatim.
print-error = Could not print: { $error }

## Printing and sharing: Windows only

print-preparing-status = Preparing to print...
print-unavailable = Printing is unavailable on this Windows installation.
# $error (String) - technical detail from Windows, kept verbatim.
print-preview-failed = Could not prepare the print preview: { $error }
# $page (Number) - one-based page number.
print-page-blank = Could not render page { $page } for printing. It will be blank.
share-open-first = Open a PDF before sharing.
share-unavailable = Sharing is unavailable on this Windows installation.
# $error (String) - technical detail from Windows, kept verbatim.
share-failed = Could not share: { $error }
share-no-document = There is no document to share.

## Write chains: signature warning before a save rewrites a signed document

write-signature-title = Saving will break this document's signature
write-signature-detail =
    This document is signed. Saving rewrites the file, so the signature will no longer match what it covers.

    It is not removed: the saved file still carries the signature, and PDF readers will report it as invalid rather than missing.

    To keep a copy that still verifies, cancel and save to a different file.
write-signature-cancel = Cancel
write-signature-proceed = Save anyway

## Save: refusals and progress

save-cancelled = Save cancelled.

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

## Lifecycle, save, share and text copy: Windows only

open-resolve-inline-edit = Resolve or cancel the inline text edit before continuing.
open-password-placeholder = Password
# Accessible name of the password entry.
open-password-name = Document password
save-done-editable = PDF saved. Changes remain editable in this session.
share-cancelled = Share cancelled.
write-signature-share-title = Sharing will break this document's signature
write-signature-share-detail =
    This document is signed. Sharing sends a rewritten copy, so its signature will no longer match what it covers.

    It is not removed: the copy still carries the signature, and PDF readers will report it as invalid rather than missing.

    To send a copy that still verifies, cancel and share the original file instead.
write-signature-share-proceed = Share anyway
selection-copy-select-first = Select text on a page before copying.
selection-text-copied = Text copied.
selection-copy-failed = Could not copy the selected text.
