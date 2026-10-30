import SwiftUI

/// Owns only the panel's chrome and placement. Feature views keep their
/// actions, permission guards, sheets and field focus.
struct ToolsSidebar: View {
    @ObservedObject var model: ViewerViewModel

    var body: some View {
        VStack(spacing: 0) {
            // A segmented picker cannot wrap at the column's 220pt minimum.
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 124), alignment: .leading)], alignment: .leading, spacing: 4) {
                ForEach(ToolsTab.allCases, id: \.self) { tab in
                    Toggle(isOn: Binding(
                        get: { model.toolsTab == tab },
                        set: { _ in model.selectToolsTab(tab) }
                    )) { Label(tab.label, shellIcon: tab.icon) }
                        .toggleStyle(.button)
                        .help(tab.label)
                        .accessibilityIdentifier("tools-tab-\(tab.rawValue)")
                }
            }
            .padding(8)
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Tools pages")
            Divider()
            // Stable identities keep unsubmitted inspector drafts and each
            // page's scroll position. Inactive pages cannot receive input or
            // appear in the accessibility tree, despite staying mounted.
            ZStack(alignment: .topLeading) {
                ForEach(ToolsTab.allCases, id: \.self) { tab in
                    page(tab)
                        .opacity(model.toolsTab == tab ? 1 : 0)
                        .allowsHitTesting(model.toolsTab == tab)
                        .disabled(model.toolsTab != tab)
                        .accessibilityHidden(model.toolsTab != tab)
                }
            }
        }
        .frame(minWidth: 0, maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Color(nsColor: .windowBackgroundColor))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Tools")
        .accessibilityIdentifier("tools-sidebar")
    }

    @ViewBuilder private func page(_ tab: ToolsTab) -> some View {
        switch tab {
        case .edit:
            ScrollView { ContentEditPanel(model: model) }
        case .annotate:
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    AnnotationToolbar(store: model.store, drawSignature: model.drawSignature)
                    PropertiesPanel(store: model.store)
                }
                .padding(.vertical, 8)
            }
        case .comments:
            CommentsPanel(store: model.store)
        case .fillSign:
            ScrollView {
                VStack(spacing: 0) {
                    SigningPanel(model: model)
                    Divider()
                    FormFieldsPanel(model: model)
                }
            }
        }
    }
}
