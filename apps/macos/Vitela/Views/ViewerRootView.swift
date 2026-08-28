import SwiftUI

struct ViewerRootView: View {
    @ObservedObject var model: ViewerViewModel
    @State private var passwordInput = ""

    private static let zoomStep = 0.1

    var body: some View {
        VStack(spacing: 0) {
            toolbar
            Divider()
            if model.store.pageSlots.isEmpty {
                placeholder
            } else {
                pageList
            }
        }
        .accessibilityIdentifier("viewer-root")
        .alert("Password required", isPresented: isPresentingPasswordPrompt) {
            SecureField("Password", text: $passwordInput)
            Button("Open") {
                model.submitPassword(passwordInput)
                passwordInput = ""
            }
            Button("Cancel", role: .cancel) { passwordInput = "" }
        } message: {
            if case .error(.wrongPassword) = model.store.state {
                Text(ViewerFailure.wrongPassword.message)
            }
        }
    }

    /// True while the store is waiting on a password — covers both the first
    /// ask (`.passwordRequired`) and a retry after a wrong one
    /// (`.wrongPassword`), so the prompt reappears until the user cancels or
    /// gets it right.
    private var isPresentingPasswordPrompt: Binding<Bool> {
        Binding(
            get: {
                switch model.store.state {
                case .error(.passwordRequired), .error(.wrongPassword): return true
                default: return false
                }
            },
            set: { _ in }
        )
    }

    private var toolbar: some View {
        HStack {
            Button("Open PDF", action: model.selectDocument)
            Button("Open Sample", action: model.openSample)
            Spacer()
            Button("−") { model.store.setZoom(model.store.zoom - Self.zoomStep) }
                .accessibilityLabel("Zoom out")
            Text("\(Int((model.store.zoom * 100).rounded()))%")
                // Not `.monospacedDigit()`: that is macOS 12+, and the
                // deployment floor is 11.0. A monospaced face keeps the
                // readout from jittering as the digit count changes.
                .font(.system(.body, design: .monospaced))
            Button("+") { model.store.setZoom(model.store.zoom + Self.zoomStep) }
                .accessibilityLabel("Zoom in")
        }
        .padding()
    }

    private var placeholder: some View {
        VStack(spacing: 12) {
            Image(systemName: "doc.richtext")
                .font(.system(size: 40))
            Text(model.title)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var pageList: some View {
        ScrollView {
            LazyVStack(spacing: 16) {
                ForEach(model.store.pageSlots, id: \.index) { slot in
                    PageView(slot: slot, zoom: model.store.zoom) { model.retry(page: slot.index) }
                        // `render` is a no-op when the slot is already current,
                        // so re-appearing during scroll costs nothing. A zoom
                        // change makes every visible slot stale, which is what
                        // re-renders them at the new DPI instead of upscaling.
                        .onAppear { model.render(page: slot.index) }
                        .onChange(of: model.store.zoom) { _ in model.render(page: slot.index) }
                }
            }
            .padding()
        }
    }
}
