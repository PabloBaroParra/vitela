import AppKit
import SwiftUI
import XCTest
@testable import Vitela

/// Fill & Sign's signing sources: one scrolling page with the form controls,
/// and buttons that start the existing signing flows behind their guards.
final class SigningPanelTests: XCTestCase {
    func testShortFillSignPageScrollsSigningSourcesAndFormControlsTogether() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.fields = [FormField(id: 5, page: 0, name: "Example", rect: PdfRect(x: 100, y: 100, width: 144, height: 36),
                                   kind: .text(multiline: false, maxLength: nil), value: .text(""))]
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))
        model.store.setFormEditMode(true)
        model.store.selectFieldFromPanel(5)
        model.selectToolsTab(.fillSign)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 220, height: 300),
                              styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let host = NSHostingView(rootView: ToolsSidebar(model: model))
        window.contentView = host
        window.orderFront(nil)
        defer { window.close() }
        RunLoop.main.run(until: Date().addingTimeInterval(0.1))
        let field = try XCTUnwrap(nativeControls(host).compactMap { $0 as? NSTextField }.first { $0.placeholderString == "Name" })
        let scroll = try XCTUnwrap(field.enclosingScrollView)
        let document = try XCTUnwrap(scroll.documentView)
        XCTAssertTrue(scrollViews(document).isEmpty, "Fill & Sign must not nest a separate scrolling form")
        XCTAssertGreaterThan(document.frame.height, scroll.contentView.bounds.height)
        let initial = scroll.contentView.bounds.origin
        scroll.contentView.scroll(to: NSPoint(x: 0, y: document.frame.height - scroll.contentView.bounds.height))
        scroll.reflectScrolledClipView(scroll.contentView)
        host.layoutSubtreeIfNeeded()
        XCTAssertNotEqual(scroll.contentView.bounds.origin, initial, "the short tools page must be scrollable")
        XCTAssertEqual(scroll.contentView.bounds.maxY, document.frame.height, accuracy: 1)
    }

    func testSigningSourceButtonsUseExistingFlowsAndRefusalGuards() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        var certificates = 0
        var tokens = 0
        let signing = SigningPrompts(
            chooseCertificate: { certificates += 1; return nil },
            askPassword: { _ in nil }, chooseIdentity: { _ in nil },
            chooseTokenModule: { tokens += 1; return nil },
            openKeychain: { throw EditFailure.failed("Test Keychain reached.") })
        let model = ViewerViewModel(store: ViewerStore(client: client), signingPrompts: signing)
        model.store.open(bytes: Data([1]))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 220, height: 500),
                              styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        // Hosted SwiftUI buttons are virtual on newer macOS releases and
        // their AX nodes need not materialize in a unit-test host. A native
        // test style exposes the real Button actions and enabled environment;
        // the layout tests below still exercise the shipped button style.
        let host = NSHostingView(rootView: SigningPanel(model: model).buttonStyle(SigningTestButtonStyle()))
        window.contentView = host
        window.orderFront(nil)
        defer { window.close() }
        func button(_ index: Int) throws -> NSButton {
            RunLoop.main.run(until: Date().addingTimeInterval(0.1))
            let buttons = nativeControls(host).compactMap { $0 as? NSButton }
            XCTAssertEqual(buttons.count, 4)
            return try XCTUnwrap(buttons.indices.contains(index) ? buttons[index] : nil)
        }
        let certificate = try button(1)
        let keychain = try button(2)
        let token = try button(3)
        XCTAssertTrue(certificate.isEnabled)
        certificate.performClick(nil)
        token.performClick(nil)
        keychain.performClick(nil)
        waitUntil({ model.store.editStatus == "Test Keychain reached." }, "the Keychain button never used its existing adapter")
        XCTAssertEqual(certificates, 1)
        XCTAssertEqual(tokens, 1)
        model.isSaving = true
        for index in 1...3 { XCTAssertFalse(try button(index).isEnabled) }
        model.isSaving = false
        model.store.signingRefusal = "This document does not permit adding signature fields."
        for index in 1...3 { XCTAssertFalse(try button(index).isEnabled) }
        XCTAssertTrue(try button(0).isEnabled, "digital signing refusal must not disable a permitted visual mark")
        host.rootView = SigningPanel(model: ViewerViewModel(store: ViewerStore(client: client))).buttonStyle(SigningTestButtonStyle())
        for index in 0...3 { XCTAssertFalse(try button(index).isEnabled) }
        XCTAssertTrue(client.signedAs.isEmpty)
    }

    private func scrollViews(_ view: NSView) -> [NSScrollView] {
        ((view as? NSScrollView).map { [$0] } ?? []) + view.subviews.flatMap(scrollViews)
    }

    private func nativeControls(_ view: NSView) -> [NSControl] {
        ((view as? NSControl).map { [$0] } ?? []) + view.subviews.flatMap(nativeControls)
    }
}

private struct SigningTestButtonStyle: PrimitiveButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        SigningTestButton(action: configuration.trigger)
    }
}

private struct SigningTestButton: NSViewRepresentable {
    @Environment(\.isEnabled) private var isEnabled
    let action: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> NSButton {
        NSButton(title: "Test signing action", target: context.coordinator, action: #selector(Coordinator.press))
    }

    func updateNSView(_ button: NSButton, context: Context) {
        button.isEnabled = isEnabled
        context.coordinator.action = action
    }

    final class Coordinator: NSObject {
        var action: () -> Void = {}
        @objc func press() { action() }
    }
}
