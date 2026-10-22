import Foundation

/// One tile of Home's Tools grid — Linux's `home/tools.rs` table, in its
/// reading order and with its wording.
enum HomeTool: CaseIterable {
    case edit, annotate, sign, organize, compress, protect

    var label: String {
        switch self {
        case .edit: return "Edit"
        case .annotate: return "Annotate"
        case .sign: return "Sign"
        case .organize: return "Organize"
        case .compress: return "Compress"
        case .protect: return "Protect"
        }
    }

    var description: String {
        switch self {
        case .edit: return "Retype text and replace images"
        case .annotate: return "Highlight, draw, and add notes"
        case .sign: return "Sign with a certificate, card, or token"
        case .organize: return "Reorder and delete pages"
        case .compress: return "Write a smaller copy of the file"
        case .protect: return "Require a password to open the document"
        }
    }

    /// An SF Symbol standing in for Linux's drawn icon.
    var symbol: String {
        switch self {
        case .edit: return "pencil"
        case .annotate: return "highlighter"
        case .sign: return "signature"
        case .organize: return "square.grid.2x2"
        case .compress: return "arrow.down.right.and.arrow.up.left"
        case .protect: return "lock"
        }
    }

    /// What the status line says when Annotate lands: the tools are the
    /// Annotate bar's, and arming one for the reader would place a mark they
    /// did not ask for — Linux's rule that a tile navigates, never edits.
    static let annotatePrompt = "Choose a tool in the Annotate bar, then drag on a page."

    /// The tiles Home's search leaves showing: those whose label contains
    /// `query`, ignoring case — Linux's `ToolsCard::filter` and Windows'
    /// `ApplyHomeFilter`. Trimmed like the Recent filter the same field
    /// drives, so the two lists never disagree about a stray space.
    static func matching(_ query: String) -> [HomeTool] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        return allCases.filter { needle.isEmpty || $0.label.localizedCaseInsensitiveContains(needle) }
    }
}

/// Home's Tools grid on macOS. A tile is a destination: with a document open
/// it goes straight to the tool; with none it opens the file chooser first
/// and goes there once the picked file has loaded, so "Sign" from a cold
/// start is one gesture — Linux's `open_tool` / `apply`.
extension ViewerViewModel {
    func openHomeTool(_ tool: HomeTool) {
        guard store.document != nil else {
            pendingHomeTool = tool
            selectDocument()
            return
        }
        returnToDocument()
        applyHomeTool(tool)
    }

    /// Home over the open document — Linux's and Windows' Home rail button.
    /// Nothing about the document changes: not its edits, not its history.
    func showHome() {
        guard store.document != nil else { return }
        showsHome = true
    }

    /// Back from Home to the document, at the page it was left on — the page
    /// list is rebuilt, so it is asked to scroll there again.
    func returnToDocument() {
        guard showsHome else { return }
        showsHome = false
        let page = store.currentPage
        DispatchQueue.main.async { [weak self] in self?.store.goToPage(page) }
    }

    /// What each tile means. Edit, Annotate and Organize only change what is
    /// shown; Sign, Compress and Protect start their own flows, which report
    /// their own refusals — as on Linux.
    func applyHomeTool(_ tool: HomeTool) {
        switch tool {
        case .edit: setContentEditMode(true)
        case .annotate: store.editStatus = HomeTool.annotatePrompt
        case .sign: signWithCertificateFile()
        case .organize: isOrganizing = true
        case .compress: requestCompress()
        case .protect: requestProtect()
        }
    }

    /// Called with each new state: a pending tool is applied once its
    /// document has loaded, kept through a password prompt, and dropped on
    /// anything else (a cancelled chooser leaves the state as it was, so the
    /// chooser's own cancel clears it — see `selectDocument`).
    func applyPendingHomeTool(_ state: ViewerState) {
        guard let tool = pendingHomeTool else { return }
        switch state {
        case .loaded:
            pendingHomeTool = nil
            // The sink sees the state before the store has finished setting
            // it, so the tool lands on the next turn of the run loop.
            DispatchQueue.main.async { [weak self] in self?.applyHomeTool(tool) }
        case .loading, .error(.passwordRequired), .error(.wrongPassword):
            break
        case .empty, .error:
            pendingHomeTool = nil
        }
    }
}
