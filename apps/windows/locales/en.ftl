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
