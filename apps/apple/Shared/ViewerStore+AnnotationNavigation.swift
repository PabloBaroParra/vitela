// Previous/next annotation: step the selection through this session's
// annotations in the core's order, wrapping at either end, and bring the one
// selected into view. Mirrors Windows' `MainWindow.AnnotationNavigation.cs`
// with `Viewer/AnnotationSelection.cs`, and the Linux "Previous annotation"
// button (`annotations/edit.rs`). Selecting is not an edit: nothing reaches
// the edit log.
import Foundation

extension ViewerStore {
    /// Whether there is an annotation to step to.
    var canStepAnnotations: Bool { document != nil && !annotations.isEmpty }

    /// Selects the annotation after (`forward`) or before the selected one
    /// and reveals it. With nothing selected, forward starts at the first and
    /// backward at the last, as on Windows.
    func stepAnnotation(forward: Bool) {
        guard annotationGesture == nil,
              let id = Self.steppedAnnotationId(annotations.map(\.id), from: selectedAnnotationId, forward: forward),
              let annotation = annotations.last(where: { $0.id == id }),
              pageSlots.indices.contains(annotation.page)
        else { return }
        selectedAnnotationId = id
        editStatus = "Selected annotation \(id) on page \(annotation.page + 1)."
        goToPage(annotation.page, anchor: revealAnchor(of: annotation))
    }

    /// Windows' `AnnotationSelection.StepId`: the neighbour of `selected` in
    /// `ids`, wrapping around; the first or last when nothing is selected;
    /// `nil` when the list is empty or no longer holds the selection.
    static func steppedAnnotationId(_ ids: [UInt64], from selected: UInt64?, forward: Bool) -> UInt64? {
        guard !ids.isEmpty else { return nil }
        guard let selected else { return forward ? ids.first : ids.last }
        guard let index = ids.firstIndex(of: selected) else { return nil }
        return ids[(index + (forward ? 1 : ids.count - 1)) % ids.count]
    }

    /// Where the annotation's centre sits on its drawn page, as fractions of
    /// the page's width and height from the top-left — placed with the
    /// core's turn-aware math, as the overlay draws it, so a rotated page
    /// reveals the right spot. `nil` for an annotation without bounds.
    private func revealAnchor(of annotation: Annotation) -> PageAnchor? {
        guard let bounds = annotation.bounds, let placement = placement(ofPage: annotation.page, zoom: 1) else { return nil }
        // The placement's size already has the turn applied.
        guard placement.width > 0, placement.height > 0 else { return nil }
        let placed = client.placeRect(bounds, on: placement)
        return PageAnchor(
            x: min(max((placed.left + placed.width / 2) / placement.width, 0), 1),
            y: min(max((placed.top + placed.height / 2) / placement.height, 0), 1)
        )
    }
}
