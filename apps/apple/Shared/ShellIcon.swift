// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// The shared drawings in `assets/icons`, one case per file — Linux's
/// `icons.rs` `Icon` and Windows' `Viewer/ShellIcons.cs` `ShellIcon`. Each
/// file is a 24×24 stroke drawing in black; a shell draws it as a template
/// and tints it with whatever colour the control's text has.
enum ShellIcon: String, CaseIterable {
    case home, recent, files, edit, annotate, sign, organize, compress, protect
    case newFile = "new-file", sample, delete, text, image, save, print, share
    case exportImages = "export-images", undo, redo
    case zoomOut = "zoom-out", zoomIn = "zoom-in", fitWidth = "fit-width", fitPage = "fit-page"
    case panelLeft = "panel-left", panelRight = "panel-right", previous, next
    case search, comments, moveUp = "move-up", moveDown = "move-down"
    case rotateLeft = "rotate-left", rotateRight = "rotate-right"
    case firstPage = "first-page", lastPage = "last-page", goToPage = "go-to-page"
    case extractPages = "extract-pages", splitPages = "split-pages"
    case select, highlight, underline, strikeout, ink, shape

    /// The folder the app bundle carries `assets/icons` in.
    static let bundleFolder = "icons"

    /// The drawing's name in `assets/icons`, without `.svg`.
    var fileName: String { rawValue }

    /// The drawing inside `bundle`, or `nil` if it was not bundled.
    func url(in bundle: Bundle = .main) -> URL? {
        bundle.url(forResource: fileName, withExtension: "svg", subdirectory: Self.bundleFolder)
    }
}
