// Annotations this session adds: the tools, a placement in progress, and the
// core's list of what has been added. Mirrors Windows'
// `MainWindow.Annotations.cs` and the Linux `annotations/` module: a tool is
// armed, a drag on a page traces its rect, releasing commits ONE undoable
// edit through the core and disarms the tool. The core owns the list; this
// re-reads it after every edit, undo and redo rather than mirroring its rules.
import Foundation

/// The tools a drag on a page can place.
enum AnnotationTool: Equatable, CaseIterable {
    case highlight, underline, strikeout, shape, ink, note

    var kind: AnnotationKind {
        switch self {
        case .highlight: return .highlight
        case .underline: return .underline
        case .strikeout: return .strikeout
        case .shape: return .shape
        case .ink: return .ink
        case .note: return .textNote
        }
    }

    /// Windows' tool names, which its status line repeats.
    var name: String {
        switch self {
        case .highlight: return "Highlight"
        case .underline: return "Underline"
        case .strikeout: return "Strikeout"
        case .shape: return "Shape"
        case .ink: return "Ink"
        case .note: return "Note"
        }
    }
}

/// A note traced on a page, waiting for its text — Windows asks for the
/// contents after the drag, and only then records the edit.
struct PendingNote: Equatable {
    let page: Int
    let rect: PdfRect
}

/// A placement being traced: where the drag started and where it is now.
struct AnnotationDraft: Equatable {
    let tool: AnnotationTool
    let page: Int
    let origin: PdfPoint
    var current: PdfPoint {
        didSet { trace(current) }
    }
    /// Every distinct position an ink stroke passed through — Windows'
    /// `InkPointerTrace`. A press and release in place stays one sample;
    /// returning to an earlier position is kept, since a closed stroke is a
    /// real drag.
    private(set) var points: [PdfPoint]

    init(tool: AnnotationTool, page: Int, origin: PdfPoint, current: PdfPoint) {
        self.tool = tool
        self.page = page
        self.origin = origin
        self.current = current
        points = [origin]
        trace(current)
    }

    private mutating func trace(_ point: PdfPoint) {
        guard tool == .ink, points.last != point else { return }
        points.append(point)
    }
}

extension ViewerStore {
    /// Floor for each side of a traced rect — Linux's `MIN_TRACED_PT`.
    static let minimumTracedPoints = 4.0
    /// An underline or strikeout is a rule this thick, on the drag's line.
    static let ruleThickness = 2.0

    /// Arms `tool`, or returns to pointer mode with `nil`.
    func armAnnotationTool(_ tool: AnnotationTool?) {
        guard document != nil, annotationEditingAllowed || tool == nil else { return }
        armedTool = tool
        annotationDraft = nil
        editStatus = tool.map { "\($0.name) armed. Drag on a page to place it." } ?? "Pointer mode."
    }

    /// Starts tracing at `point` on `page` when a tool is armed. Returns
    /// whether the drag is an annotation's — when it is not, the shell goes
    /// on to select text.
    func beginAnnotationDraft(page: Int, at point: PdfPoint) -> Bool {
        guard let tool = armedTool, annotationEditingAllowed, pageSlots.indices.contains(page) else { return false }
        annotationDraft = AnnotationDraft(tool: tool, page: page, origin: point, current: point)
        return true
    }

    func continueAnnotationDraft(page: Int, to point: PdfPoint) {
        guard annotationDraft?.page == page else { return }
        annotationDraft?.current = point
    }

    /// Commits the traced annotation as one undoable edit and disarms the
    /// tool, as both other shells do after a placement.
    func endAnnotationDraft(page: Int, at point: PdfPoint) {
        guard var draft = annotationDraft, draft.page == page else { return }
        draft.current = point
        annotationDraft = nil
        armedTool = nil
        if draft.tool == .ink {
            // A tap leaves a single point, which is not a stroke: say so
            // rather than save an invisible annotation (Linux's
            // `INK_NEEDS_A_DRAG`, Windows' `CommitPlacementAsync`).
            guard draft.points.count >= 2 else {
                editStatus = "Ink needs a drag, not a tap."
                return
            }
            return applyAnnotationEdit(.addInk(page: page, points: draft.points, color: .annotationDefault))
        }
        let rect = Self.tracedRect(from: draft.origin, to: draft.current, tool: draft.tool)
        if draft.tool == .note {
            pendingNote = PendingNote(page: page, rect: rect)
            return
        }
        applyAnnotationEdit(.add(kind: draft.tool.kind, page: page, rect: rect, color: .annotationDefault))
    }

    /// Records the pending note with `text`. Blank text records nothing: the
    /// shell keeps its Add button disabled until there is some.
    func commitPendingNote(text: String) {
        guard let note = pendingNote, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        pendingNote = nil
        guard annotationEditingAllowed, pageSlots.indices.contains(note.page) else { return }
        applyAnnotationEdit(.addNote(page: note.page, rect: note.rect, contents: text))
    }

    func cancelPendingNote() {
        guard pendingNote != nil else { return }
        pendingNote = nil
        editStatus = "Note placement canceled."
    }

    func cancelAnnotationDraft() {
        annotationDraft = nil
    }

    /// The rect a drag traces — Windows' `NormalizedRect`: corner to corner
    /// with a floor on each side, except that an underline or strikeout is a
    /// thin rule along the line the drag started on.
    static func tracedRect(from origin: PdfPoint, to current: PdfPoint, tool: AnnotationTool) -> PdfRect {
        let width = max(minimumTracedPoints, abs(current.x - origin.x))
        let isRule = tool == .underline || tool == .strikeout
        return PdfRect(
            x: min(origin.x, current.x),
            y: isRule ? origin.y : min(origin.y, current.y),
            width: width,
            height: isRule ? ruleThickness : max(minimumTracedPoints, abs(current.y - origin.y))
        )
    }

    func annotations(onPage page: Int) -> [Annotation] {
        annotations.filter { $0.page == page }
    }

    /// Where `page` is drawn at the current zoom, for overlays and pointers.
    func placement(ofPage page: Int, zoom: Double) -> PagePlacement? {
        guard pageSlots.indices.contains(page) else { return nil }
        let dimensions = pageSlots[page].dimensions
        return PagePlacement(width: dimensions.width, height: dimensions.height, rotation: dimensions.rotation, scale: zoom)
    }

    func applyAnnotationEdit(_ edit: AnnotationEdit) {
        guard let document else { return }
        do {
            try client.applyAnnotationEdit(document: document, edit: edit)
        } catch {
            editStatus = Self.editMessage(for: error)
            return
        }
        refreshAnnotations()
        recordEdit(status: "Annotation edited. Changes are pending save.")
    }

    /// Re-reads the core's list — after an edit, an undo or a redo.
    func refreshAnnotations() {
        guard let document else {
            annotations = []
            return
        }
        annotations = client.annotations(document: document)
    }

    /// Called by `resetEditing` once a document has replaced the previous one.
    func resetAnnotations() {
        armedTool = nil
        annotationDraft = nil
        pendingNote = nil
        annotationEditingAllowed = document.map { client.annotationEditingAllowed(document: $0) } ?? false
        refreshAnnotations()
    }
}
