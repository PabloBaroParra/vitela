import SwiftUI

/// The two passwords — Windows' "Protect with a password" dialog. The
/// fields are cleared whenever the sheet goes away, so neither password
/// outlives the request it was typed for.
struct ProtectSheet: View {
    @ObservedObject var model: ViewerViewModel
    @State private var openPassword = ""
    @State private var permissionsPassword = ""
    @State private var problem: String?

    private enum Field { case open, permissions }
    @FocusState private var focus: Field?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Protect with a password").font(.headline)
            SecureField("Password to open the document", text: $openPassword)
                .focused($focus, equals: .open)
                .onSubmit { focus = .permissions }
            SecureField("Permissions password", text: $permissionsPassword)
                .focused($focus, equals: .permissions)
                .onSubmit(protect)
                .help("Governs what readers may do with the document once it is open, and lifts those restrictions for whoever holds it. It must not be the same as the open password.")
            if let problem {
                Text(problem)
                    .foregroundColor(.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("protect-problem")
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel, action: model.cancelProtect)
                    .keyboardShortcut(.cancelAction)
                Button("Protect", action: protect)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .textFieldStyle(.roundedBorder)
        .padding()
        .frame(width: 380)
        .onAppear { focus = .open }
        .onDisappear {
            openPassword = ""
            permissionsPassword = ""
        }
    }

    private func protect() {
        problem = model.protect(openPassword: openPassword, permissionsPassword: permissionsPassword)
    }
}
