import SwiftUI

/// Persistent navigation beside both Home and the editor, as on Linux and
/// Windows. Only its presentation collapses on a narrow window; all commands
/// stay reachable, named for VoiceOver, and in the same keyboard order.
struct AppRail: View {
    @ObservedObject var model: ViewerViewModel
    var compact: Bool
    @State private var hovered: ShellSection?

    static let expandedWidth: CGFloat = 176
    static let compactWidth: CGFloat = 56
    static let collapseBelowWidth: CGFloat = 1000

    var body: some View {
        ScrollView(.vertical) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    AppMarkView(edge: 24)
                    if !compact { Text("Vitela").font(.title3.weight(.semibold)) }
                }
                .padding(.horizontal, 4)
                .frame(maxWidth: .infinity, alignment: compact ? .center : .leading)
                .padding(.bottom, 12)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("Vitela")

                ForEach(ShellSection.allCases, id: \.self) { section in
                    if section == .edit { Divider().padding(.vertical, 6) }
                    item(section)
                }
            }
            .padding(.horizontal, compact ? 8 : 12)
            .padding(.vertical, 14)
        }
        .frame(width: compact ? Self.compactWidth : Self.expandedWidth)
        .frame(maxHeight: .infinity)
        .background(Color(nsColor: .controlBackgroundColor))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("app-rail")
        .accessibilityLabel("Vitela sections")
    }

    private func item(_ section: ShellSection) -> some View {
        let active = model.activeShellSection == section
        return Button { model.navigateShell(to: section) } label: {
            HStack(spacing: 8) {
                ShellIconImage(icon: section.icon, size: 16)
                if !compact {
                    Text(section.label).fontWeight(active ? .semibold : .regular)
                    Spacer(minLength: 0)
                }
            }
            .frame(maxWidth: .infinity, minHeight: 32)
            .foregroundStyle(active ? Color.vitelaAccent : Color.secondary)
            .background(RoundedRectangle(cornerRadius: 6).fill(active || hovered == section ? Color.vitelaAccent.opacity(0.12) : Color.clear))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(section.label)
        .accessibilityLabel(section.label)
        .accessibilityIdentifier("rail-\(section.rawValue)")
        .accessibilityAddTraits(active ? [.isSelected] : [])
        .disabled(!model.canNavigateShell)
        .onHover { hovered = $0 ? section : nil }
    }
}
