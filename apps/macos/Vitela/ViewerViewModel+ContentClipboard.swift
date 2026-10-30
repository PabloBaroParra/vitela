import AppKit
import SwiftUI

/// The page and point the pointer last crossed in Edit Content, PDF space.
struct ContentPointer: Equatable {
    let page: Int
    let point: PdfPoint
}

/// ⌘C, ⌘X and ⌘V in Edit Content (B25 T-209) — Windows'
/// `MainWindow.ContentClipboard.cs`: copy or cut what is selected, paste it
/// with its top-left corner where the pointer is, on any page or in another
/// document. The rules are the store's (`ViewerStore+ContentClipboard.swift`).
extension ViewerViewModel {
    func trackContentPointer(page: Int, location: CGPoint, zoom: Double) {
        guard store.contentEditMode, let point = annotationPoint(page: page, location: location, zoom: zoom) else { return }
        contentPointer = ContentPointer(page: page, point: point)
    }

    /// Takes ⌘C/⌘X with something selected and ⌘V with a clip held, outside
    /// a text field; `false` leaves the key to text copy and image paste.
    func handleContentClipboardKey(_ event: NSEvent) -> Bool {
        guard store.contentEditMode, !isOrganizing,
              event.modifierFlags.intersection(.deviceIndependentFlagsMask) == .command,
              !(event.window?.firstResponder is NSText)
        else { return false }
        switch event.charactersIgnoringModifiers {
        case "c" where store.clipboardSelection != nil: copyContent()
        case "x" where store.clipboardSelection != nil: cutContent()
        case "v" where contentClip != nil: pasteContent()
        default: return false
        }
        return true
    }

    func copyContent(pasteboard: NSPasteboard = .general) {
        guard let (clip, selection) = store.copyContentSelection() else { return }
        hold(clip, selection, pasteboard: pasteboard)
    }

    func cutContent(pasteboard: NSPasteboard = .general) {
        guard let selection = store.clipboardSelection, let (clip, request) = store.beginContentCut() else { return }
        hold(clip, selection, pasteboard: pasteboard)
        enqueuePageWork(request)
    }

    /// Where the pointer last was over a page; with none seen, on the page
    /// in view where the clip was copied from — Windows' fallback.
    func pasteContent() {
        guard let clip = contentClip, !store.pageSlots.isEmpty else { return }
        let target = contentPointer.flatMap { store.pageSlots.indices.contains($0.page) ? $0 : nil }
            ?? ContentPointer(page: min(store.currentPage, store.pageSlots.count - 1),
                              point: PdfPoint(x: clip.bbox.x, y: clip.bbox.y + clip.bbox.height))
        enqueuePageWork(store.beginContentPaste(clip, page: target.page, at: target.point))
    }

    /// Keeps the clip for ⌘V, and the selected text on the system clipboard
    /// so it pastes as text in other apps.
    private func hold(_ clip: ContentClip, _ selection: ContentSelection, pasteboard: NSPasteboard) {
        contentClip = clip
        let text = selection.blocks.map(\.text).joined(separator: "\n")
        guard !text.isEmpty else { return }
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
    }
}

extension View {
    /// Reports where the pointer hovers in this view's coordinates; macOS 12
    /// has no continuous hover, so there a paste uses the last press.
    func trackingContentPointer(_ moved: @escaping (CGPoint) -> Void) -> some View {
        modifier(ContentPointerTracking(moved: moved))
    }
}

private struct ContentPointerTracking: ViewModifier {
    let moved: (CGPoint) -> Void

    func body(content: Content) -> some View {
        if #available(macOS 13.0, *) {
            content.onContinuousHover { phase in
                if case let .active(location) = phase { moved(location) }
            }
        } else {
            content
        }
    }
}
