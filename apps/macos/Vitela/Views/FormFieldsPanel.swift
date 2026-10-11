import SwiftUI

/// Every form field as a control, built from the core's list — Windows'
/// Forms tool page and the GTK fill panel. Each control is built so it cannot
/// offer an invalid value (a capped length, the real options); the core
/// still validates what arrives.
struct FormFieldsPanel: View {
    @ObservedObject var model: ViewerViewModel

    private var store: ViewerStore { model.store }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Form").font(.headline)
            if store.formStructureAllowed {
                Toggle("Edit fields", isOn: Binding(get: { store.formEditMode }, set: store.setFormEditMode))
                    .toggleStyle(.button)
                    .help("Select, move, resize, rename, restyle or delete fields on the page")
                    .disabled(store.document == nil)
                if store.formEditMode, let selected = store.selectedField {
                    FormFieldInspector(model: model, field: selected)
                    Divider()
                }
                HStack(spacing: 4) {
                    ForEach(FieldToPlace.allCases, id: \.self) { kind in
                        Toggle(kind.name, isOn: Binding(
                            get: { store.armedField == kind },
                            set: { store.armField($0 ? kind : nil) }
                        ))
                        .toggleStyle(.button)
                        .help("Click or drag on a page to place a \(kind.name.lowercased())")
                    }
                }
                .disabled(store.document == nil || store.pageWorkInFlight)
            }
            if store.formFields.isEmpty {
                Text("Open a PDF with form fields to fill them in, or place one above.").foregroundStyle(.secondary)
            }
            if !store.formStatus.isEmpty {
                Text(store.formStatus).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    ForEach(store.formFields, id: \.id) { field in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(field.name.isEmpty ? "Unnamed field" : field.name)
                                .font(.caption).foregroundStyle(.secondary)
                            FormFieldControl(field: field) { model.fillField(field.id, value: $0) }
                                .disabled(!store.formFillAllowed || store.pageWorkInFlight)
                        }
                        // A new value from the core rebuilds the row; typing
                        // into it does not, so focus stays put.
                        .id("\(field.id)-\(String(describing: field.value))")
                    }
                }
            }
        }
        .padding()
        .accessibilityIdentifier("form-panel")
    }
}

/// One field's control. Text commits on Return and when focus leaves it, not
/// on every keystroke: a fill re-derives the whole preview, so per-key
/// commits would queue one rebuild per character.
private struct FormFieldControl: View {
    let field: FormField
    let commit: (FieldValue) -> Void

    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        switch field.kind {
        case let .text(multiline, maxLength):
            textControl(multiline: multiline, maxLength: maxLength)
        case .checkbox:
            Toggle(field.name, isOn: Binding(
                get: { field.value == .checked(true) },
                set: { commit(.checked($0)) }
            ))
            .labelsHidden()
        case let .radioGroup(options):
            Picker(field.name, selection: Binding(
                get: { FormFieldChoices.index(of: chosen, in: options) },
                set: { commit(.choice(FormFieldChoices.choice(at: $0, in: options))) }
            )) {
                ForEach(Array(options.enumerated()), id: \.offset) { index, option in
                    Text(option).tag(index + 1)
                }
            }
            .pickerStyle(.radioGroup)
            .labelsHidden()
        case let .dropdown(options, editable: false):
            Picker(field.name, selection: Binding(
                get: { FormFieldChoices.index(of: chosen, in: options) },
                set: { commit(.choice(FormFieldChoices.choice(at: $0, in: options))) }
            )) {
                Text(FormFieldChoices.noChoiceLabel).tag(0)
                ForEach(Array(options.enumerated()), id: \.offset) { index, option in
                    Text(option).tag(index + 1)
                }
            }
            .labelsHidden()
        case let .dropdown(options, editable: true):
            // Accepts text outside its options, so it is typed into; the
            // options are named as the placeholder — as on Windows and GTK.
            TextField(options.joined(separator: ", "), text: $text)
                .textFieldStyle(.roundedBorder)
                .focused($focused)
                .onAppear { text = chosen ?? "" }
                .onSubmit(commitChoiceText)
                .onChange(of: focused) { if !$0 { commitChoiceText() } }
        }
    }

    private var chosen: String? {
        if case let .choice(option) = field.value { return option }
        return nil
    }

    @ViewBuilder
    private func textControl(multiline: Bool, maxLength: Int?) -> some View {
        let binding = Binding(get: { text }, set: { text = FormFieldText.capped($0, maxLength: maxLength) })
        Group {
            if multiline {
                TextEditor(text: binding)
                    .frame(minHeight: 60)
                    .border(Color.secondary.opacity(0.4))
            } else {
                TextField("", text: binding)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(commitText)
            }
        }
        .focused($focused)
        .disabled(maxLength == 0)
        .onAppear {
            if case let .text(value) = field.value { text = value }
        }
        .onChange(of: focused) { if !$0 { commitText() } }
    }

    private func commitText() {
        commit(.text(FormFieldText.fromControl(text)))
    }

    private func commitChoiceText() {
        commit(.choice(text.isEmpty ? nil : text))
    }
}
