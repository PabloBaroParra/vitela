import Foundation

/// Typed positions and sizes for page content on macOS: a line's X/Y from
/// its Edit text sheet, and the selected image's Position… and Size… through
/// the shared `GeometrySheet`. The rules are the store's
/// (`ViewerStore+ContentGeometry.swift`).
extension ViewerViewModel {
    /// The Edit text sheet's Move: `run` to `x`, `y`. Returns why the sheet
    /// must stay open, or `nil` once it is closed.
    func moveText(_ run: ContentTextRun, toX x: Double, y: Double) -> String? {
        let (problem, request) = store.beginTextMove(run, toX: x, y: y)
        if let problem { return problem }
        contentTextEditor = nil
        enqueuePageWork(request)
        return nil
    }

    /// The selected image's Position… or Size…; `nil` with no image selected.
    func selectedImageGeometry(position: Bool) -> GeometrySheet.Request? {
        guard let image = store.selectedContentImage else { return nil }
        let page = image.page + 1
        if position {
            return GeometrySheet.Request(
                title: "Move image — page \(page)", labels: ("X", "Y"), values: (image.bbox.x, image.bbox.y),
                hint: ViewerStore.ContentGeometry.imageHint, action: "Move"
            ) { [weak self] x, y in
                self?.start(self?.store.beginSelectedImageMove(toX: x, y: y))
            }
        }
        return GeometrySheet.Request(
            title: "Resize image — page \(page)", labels: ("Width", "Height"), values: (image.bbox.width, image.bbox.height),
            hint: ViewerStore.ContentGeometry.imageHint, action: "Resize"
        ) { [weak self] width, height in
            self?.start(self?.store.beginSelectedImageResize(width: width, height: height))
        }
    }

    /// Runs the edit a sheet's answer carries, or hands back its problem.
    private func start(_ answer: (problem: String?, request: ViewerStore.PageWorkRequest?)?) -> String? {
        guard let answer else { return nil }
        if answer.problem == nil { enqueuePageWork(answer.request) }
        return answer.problem
    }
}
