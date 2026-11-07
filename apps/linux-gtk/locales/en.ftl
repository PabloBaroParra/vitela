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
