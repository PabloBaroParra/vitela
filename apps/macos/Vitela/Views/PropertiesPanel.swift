import SwiftUI

/// The document's Info dictionary as editable fields — the macOS twin of the
/// GTK `metadata.rs` panel and Windows' `MainWindow.Metadata.cs`. Text
/// properties commit on every change, as theirs do; dates commit on Return
/// or when the field loses focus, because a half-typed date is not a date.
struct PropertiesPanel: View {
    @ObservedObject var store: ViewerStore

    @State private var createdText = ""
    @State private var modifiedText = ""
    @FocusState private var focusedDate: ViewerStore.DocumentDateProperty?

    var body: some View {
        Form {
            Section {
                textRow("Title", .title)
                textRow("Author", .author)
                textRow("Subject", .subject)
                textRow("Keywords", .keywords)
                textRow("Creator", .creator)
                textRow("Producer", .producer)
                dateRow("Created", .created, text: $createdText)
                dateRow("Modified", .modified, text: $modifiedText)
                Text("Pages: \(store.pageSlots.isEmpty ? "—" : String(store.pageSlots.count))")
                    .foregroundStyle(.secondary)
            } header: {
                Text("Properties").font(.headline)
            } footer: {
                if store.documentInfo != nil && !store.editingAllowed {
                    Text("This document does not permit metadata changes.")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .disabled(store.documentInfo == nil || !store.editingAllowed)
        .onAppear(perform: reloadDates)
        .onChange(of: store.documentInfo) { _ in reloadDates() }
        .onChange(of: focusedDate) { [focusedDate] _ in
            // `focusedDate` in the capture list is the value *before* the
            // change: the field that just lost focus is the one to commit.
            if let left = focusedDate { commitDate(left) }
        }
        .accessibilityIdentifier("properties-panel")
    }

    private func textRow(_ label: String, _ property: ViewerStore.DocumentProperty) -> some View {
        TextField(label, text: Binding(
            get: { value(of: property) },
            set: { store.setDocumentProperty(property, to: $0) }
        ))
    }

    private func dateRow(_ label: String, _ property: ViewerStore.DocumentDateProperty, text: Binding<String>) -> some View {
        TextField(label, text: text, prompt: Text("YYYY-MM-DD HH:MM:SS"))
            .focused($focusedDate, equals: property)
            .onSubmit { commitDate(property) }
    }

    private func value(of property: ViewerStore.DocumentProperty) -> String {
        guard let info = store.documentInfo else { return "" }
        switch property {
        case .title: return info.title ?? ""
        case .author: return info.author ?? ""
        case .subject: return info.subject ?? ""
        case .keywords: return info.keywords ?? ""
        case .creator: return info.creator ?? ""
        case .producer: return info.producer ?? ""
        }
    }

    private func commitDate(_ property: ViewerStore.DocumentDateProperty) {
        let text = property == .created ? createdText : modifiedText
        let stored = MetadataDateText.format(
            property == .created ? store.documentInfo?.creationDate : store.documentInfo?.modDate
        )
        guard text != stored else { return }
        // A rejected entry leaves `documentInfo` unchanged, so `onChange`
        // would never fire to put the stored value back.
        if !store.setDocumentDate(property, to: text) { reloadDates() }
    }

    /// Shows the stored dates — after an open, an undo, or a rejected entry.
    private func reloadDates() {
        createdText = MetadataDateText.format(store.documentInfo?.creationDate)
        modifiedText = MetadataDateText.format(store.documentInfo?.modDate)
    }
}
