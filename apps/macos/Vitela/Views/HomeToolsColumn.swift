import SwiftUI

/// Home's right-hand column — Linux's `home/tools.rs`: the Tools grid, the
/// Quick actions that need no document, and the keyboard reference, which
/// holds the slot a cloud app would give a storage quota this one does not
/// have.
struct HomeToolsColumn: View {
    @ObservedObject var model: ViewerViewModel
    /// Home's search, which filters the Tools grid as well as Recent.
    var query = ""
    /// Fixed beside Recent; `nil` fills the body once Home stacks it below
    /// (`HomeView.isStacked`).
    var width: CGFloat? = HomeView.toolsColumnWidth

    /// What macOS binds them to (`VitelaCommands`, the Find button) — the
    /// same five Linux lists.
    static let shortcuts = [("Open", "⌘O"), ("New", "⌘N"), ("Save As", "⌘S"), ("Find", "⌘F"), ("Print", "⌘P")]

    private let columns = Array(repeating: GridItem(.flexible(), spacing: 8), count: 3)

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            // A titled card over an empty grid reads as broken rather than
            // filtered, so the card leaves when no tile matches — as on Linux
            // and Windows.
            let tools = HomeTool.matching(query)
            if !tools.isEmpty {
                card("Tools") {
                    LazyVGrid(columns: columns, spacing: 8) {
                        ForEach(tools, id: \.self) { tool in
                            Button { model.openHomeTool(tool) } label: {
                                VStack(spacing: 6) {
                                    ShellIconImage(icon: tool.icon, size: 24)
                                        .foregroundColor(tint(tool))
                                    Text(tool.label).font(.callout)
                                }
                                .frame(maxWidth: .infinity, minHeight: 56)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.bordered)
                            .help(tool.description)
                            .accessibilityLabel(tool.label)
                            .accessibilityHint(tool.description)
                        }
                    }
                }
            }
            card("Quick actions") {
                VStack(alignment: .leading, spacing: 6) {
                    quickAction(.newFile, "New blank PDF", model.newDocument)
                    quickAction(.files, "Open file…", model.selectDocument)
                    quickAction(.sample, "Open the sample", model.openSample)
                }
            }
            card("Keyboard shortcuts") {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(Self.shortcuts, id: \.0) { name, keys in
                        HStack {
                            Text(name)
                            Spacer()
                            Text(keys).font(.system(.body, design: .monospaced)).foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
        .frame(width: width)
        .frame(maxWidth: width == nil ? .infinity : nil, alignment: .leading)
    }

    private func card<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(.headline)
            content()
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color(nsColor: .controlBackgroundColor)))
    }

    private func quickAction(_ icon: ShellIcon, _ label: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(label, shellIcon: icon)
        }
        .buttonStyle(.borderless)
    }

    private func tint(_ tool: HomeTool) -> Color {
        guard let hex = tool.tintHex else { return .vitelaAccent }
        return Color(nsColor: .vitelaTint(hex))
    }
}
