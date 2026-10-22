import Foundation

/// Turns the color panel's stream of colors into one recolor. SwiftUI's
/// `ColorPicker` reports every color the pointer crosses while dragging in
/// the panel, and each recolor is an undo step in the core; Linux and
/// Windows pick in a dialog that answers once, and Windows pins "a color
/// edit must undo in one step". So the last color is held until the panel
/// has been still for `delay`, then committed — or at once, when the reader
/// selects something else first.
final class ColorSettler: ObservableObject {
    /// How long the picked color must stay put before it is committed.
    static let delay: TimeInterval = 0.5

    @Published private(set) var pending: (id: UInt64, color: PdfColor)?

    private let schedule: (TimeInterval, DispatchWorkItem) -> Void
    private var work: DispatchWorkItem?
    private var commit: ((UInt64, PdfColor) -> Void)?

    init(schedule: @escaping (TimeInterval, DispatchWorkItem) -> Void = { delay, work in
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
    }) {
        self.schedule = schedule
    }

    /// The color being picked for annotation `id`, while it has not landed.
    func color(for id: UInt64) -> PdfColor? {
        guard let pending, pending.id == id else { return nil }
        return pending.color
    }

    /// Holds `color` for annotation `id`, committing a color still held for
    /// another annotation first.
    func pick(_ color: PdfColor, for id: UInt64, commit: @escaping (UInt64, PdfColor) -> Void) {
        if let pending, pending.id != id { flush() }
        pending = (id, color)
        self.commit = commit
        work?.cancel()
        let work = DispatchWorkItem { [weak self] in self?.flush() }
        self.work = work
        schedule(Self.delay, work)
    }

    /// Commits the held color now, if there is one.
    func flush() {
        work?.cancel()
        work = nil
        guard let pending, let commit else { return }
        self.pending = nil
        commit(pending.id, pending.color)
    }
}
