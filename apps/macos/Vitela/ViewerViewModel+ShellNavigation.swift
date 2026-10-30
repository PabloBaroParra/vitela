import Foundation

/// The two groups in Linux's `build_app_rail` and Windows' `AppRail`, in
/// the same order. Files and Protect invoke the existing native flows;
/// Sign reveals the signing sources in Fill & Sign.
enum ShellSection: String, CaseIterable {
    case home, recent, files, edit, annotate, sign, organize, protect

    var label: String {
        switch self {
        case .home: return "Home"
        case .recent: return "Recent"
        case .files: return "My files"
        case .edit: return "Edit PDF"
        case .annotate: return "Annotate"
        case .sign: return "Sign"
        case .organize: return "Organize pages"
        case .protect: return "Protect"
        }
    }

    var icon: ShellIcon {
        switch self {
        case .home: return .home
        case .recent: return .recent
        case .files: return .files
        case .edit: return .edit
        case .annotate: return .annotate
        case .sign: return .sign
        case .organize: return .organize
        case .protect: return .protect
        }
    }

    var tool: HomeTool? {
        switch self {
        case .edit: return .edit
        case .annotate: return .annotate
        case .sign: return .sign
        case .organize: return .organize
        case .protect: return .protect
        case .home, .recent, .files: return nil
        }
    }
}

extension ViewerViewModel {
    /// The root and the rail share this decision; neither owns a second
    /// copy of which workspace is on screen.
    var isShowingHome: Bool { store.pageSlots.isEmpty || showsHome }

    var canNavigateShell: Bool { !store.pageWorkInFlight && !isSaving && !isImporting }

    var showsReaderChrome: Bool { !isShowingHome && !isOrganizing }

    /// Windows' FindDocument_Invoked also leaves Organize before focusing
    /// search. Never focus a control in a workspace that is still hidden.
    @discardableResult
    func revealDocumentSearch() -> Bool {
        guard store.document != nil && canNavigateShell else { return false }
        isOrganizing = false
        returnToDocument()
        return true
    }

    /// Derive selection from the visible workspace and selected tools page,
    /// never a second navigation state — like Windows' SelectToolPage, which
    /// marks the rail with the page. Comments has no rail entry, so nothing
    /// is selected. Dialog actions do not claim a workspace.
    var activeShellSection: ShellSection? {
        if isShowingHome { return homeSection }
        if isOrganizing { return .organize }
        switch toolsTab {
        case .edit: return .edit
        case .annotate: return .annotate
        case .comments: return nil
        case .fillSign: return .sign
        }
    }

    func navigateShell(to section: ShellSection) {
        guard canNavigateShell else { return }
        switch section {
        case .home:
            showHome()
            homeSection = .home
        case .recent:
            showHome()
            homeSection = .recent
            recentFocusRequest &+= 1
        case .files:
            selectDocument()
        default:
            if let tool = section.tool { openHomeTool(tool) }
        }
    }
}
