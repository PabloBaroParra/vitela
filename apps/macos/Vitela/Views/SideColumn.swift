import AppKit
import SwiftUI

/// One side column's width and how its divider moves it — Windows'
/// `Viewer/SideColumn.cs` and the rule Linux's `side_panel.rs` gives its
/// `Paned`s: dragging the divider past the minimum folds the column rather
/// than clipping its controls, dragging back across the minimum unfolds it,
/// and a column reopened from its toggle comes back at the last width it had
/// open, not at a default.
struct SideColumn: Equatable {
    /// How far one arrow-key press moves a focused divider — Windows'
    /// `DividerKeyStep`.
    static let keyStep = 16.0

    let minimum: Double
    let maximum: Double
    /// The width the column draws at while open.
    private(set) var preferredWidth: Double
    private var dragStart = 0.0

    init(minimum: Double, maximum: Double, initial: Double) {
        self.minimum = minimum
        self.maximum = maximum
        preferredWidth = min(max(initial, minimum), maximum)
    }

    /// Starts a divider drag from what is on screen now: nothing for a
    /// folded column, its width for an open one.
    mutating func beginDrag(isOpen: Bool) {
        dragStart = isOpen ? preferredWidth : 0
    }

    /// The column `widening` points wider (negative: narrower) than when the
    /// drag began. Answers whether it is open afterwards; a fold keeps the
    /// last open width for the toggle to reopen at.
    mutating func drag(by widening: Double) -> Bool {
        let width = max(0, dragStart + widening)
        guard width >= minimum else { return false }
        preferredWidth = min(width, maximum)
        return true
    }
}

/// The draggable line between a side column and the pages. Drag it, or focus
/// it (Full Keyboard Access) and press ← / →, to resize the column; past the
/// column's minimum it folds, as Windows' `PagesDivider`/`ToolsDivider` and
/// Linux's `Paned` handles do.
struct ColumnDivider: View {
    @Binding var column: SideColumn
    /// Written when the drag folds or unfolds the column. A column that
    /// cannot fold passes a binding that ignores the write.
    @Binding var isOpen: Bool
    /// `1` when the column sits left of the divider (dragging right widens
    /// it), `-1` when it sits on the right.
    let widening: Double
    let label: String

    @State private var dragging = false
    @State private var hovering = false

    var body: some View {
        // Drawn rather than a `Divider()`, whose axis comes from the stack it
        // sits in and is lost behind these modifiers.
        Rectangle()
            .fill(Color(nsColor: .separatorColor))
            .frame(width: 1)
            .frame(maxHeight: .infinity)
            .padding(.horizontal, 3)
            .contentShape(Rectangle())
            .onHover { inside in
                guard inside != hovering else { return }
                hovering = inside
                if inside { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() }
            }
            // A drag that folds a right-hand panel removes this divider under
            // the pointer, with no hover-out to restore the cursor.
            .onDisappear {
                if hovering { NSCursor.pop() }
                hovering = false
            }
            // Global: the divider itself moves as the column resizes, so a
            // local translation would chase its own tail.
            .gesture(
                DragGesture(minimumDistance: 1, coordinateSpace: .global)
                    .onChanged { value in
                        if !dragging {
                            dragging = true
                            column.beginDrag(isOpen: isOpen)
                        }
                        move(by: widening * value.translation.width)
                    }
                    .onEnded { _ in dragging = false }
            )
            .focusable()
            .onMoveCommand { direction in
                switch direction {
                case .left: step(-1)
                case .right: step(1)
                default: break
                }
            }
            .accessibilityElement()
            .accessibilityLabel(label)
            .accessibilityHint("Drag, or press the left and right arrow keys, to resize")
            .help(label)
    }

    private func step(_ direction: Double) {
        column.beginDrag(isOpen: isOpen)
        move(by: widening * direction * SideColumn.keyStep)
    }

    private func move(by widening: Double) {
        let open = column.drag(by: widening)
        if open != isOpen { isOpen = open }
    }
}
