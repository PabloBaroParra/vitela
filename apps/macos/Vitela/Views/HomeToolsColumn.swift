import SwiftUI

/// Home's right-hand column — Linux's `home/tools.rs`: the Tools grid, the
/// Quick actions that need no document, and the keyboard reference, which
/// holds the slot a cloud app would give a storage quota this one does not
/// have.
struct HomeToolsColumn: View {
    @ObservedObject var model: ViewerViewModel
    /// Home's search, which filters the Tools grid as well as Recent.
    var query = ""

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
                                    Image(systemName: tool.symbol).font(.title2)
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
                    quickAction("doc.badge.plus", "New blank PDF", model.newDocument)
                    quickAction("folder", "Open file…", model.selectDocument)
                    quickAction("doc.text", "Open the sample", model.openSample)
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
        .frame(width: 300)
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

    private func quickAction(_ symbol: String, _ label: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(label, systemImage: symbol)
        }
        .buttonStyle(.borderless)
    }
}
