import Foundation

/// Linux's TABS and Windows' BuildToolPages order; Annotate opens by default.
enum ToolsTab: String, CaseIterable {
    case edit, annotate, comments, fillSign

    var label: String {
        switch self {
        case .edit: return "Edit"
        case .annotate: return "Annotate"
        case .comments: return "Comments"
        case .fillSign: return "Fill & Sign"
        }
    }

    var icon: ShellIcon {
        switch self {
        case .edit: return .edit
        case .annotate: return .annotate
        case .comments: return .comments
        case .fillSign: return .sign
        }
    }
}

extension ViewerViewModel {
    /// Navigation only, like Windows' SelectToolPage: no edit, I/O or mode
    /// changes. In particular, inspecting Comments must not lose an edit tool.
    func selectToolsTab(_ tab: ToolsTab) {
        toolsTab = tab
        showsTools = true
    }
}
