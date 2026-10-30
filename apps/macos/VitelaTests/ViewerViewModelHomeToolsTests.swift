import XCTest
@testable import Vitela

/// Home's Tools grid: a tile goes to its tool, and one picked with no
/// document open waits for the document chosen for it.
final class ViewerViewModelHomeToolsTests: XCTestCase {
    func testSignDestinationsRevealChoicesWithoutOpeningACertificateOrEditing() {
        let (client, model) = model(signing: SigningPrompts(
            chooseCertificate: { XCTFail("Sign navigation must not choose a source"); return nil },
            askPassword: { _ in nil }, chooseIdentity: { _ in nil }))
        let revision = model.store.editRevision
        for fromHome in [false, true] {
            model.isOrganizing = true
            model.showsTools = false
            if fromHome {
                model.showHome()
                model.openHomeTool(.sign)
            } else {
                model.navigateShell(to: .sign)
            }
            XCTAssertFalse(model.isShowingHome)
            XCTAssertFalse(model.isOrganizing)
            XCTAssertFalse(model.store.contentEditMode)
            XCTAssertNil(model.store.armedTool)
            XCTAssertTrue(model.showsTools)
            XCTAssertEqual(model.toolsTab, .fillSign)
            XCTAssertEqual(model.activeShellSection, .sign)
            XCTAssertEqual(model.store.editRevision, revision)
        }
        XCTAssertTrue(client.signedAs.isEmpty)
        XCTAssertTrue(client.pageEdits.isEmpty)
        model.navigateShell(to: .annotate)
        XCTAssertEqual(model.activeShellSection, .annotate)
        model.navigateShell(to: .edit)
        XCTAssertEqual(model.activeShellSection, .edit)
    }

    func testPendingSignRevealsSourcesAfterOpeningTheDocument() {
        let (_, model) = model(open: false, signing: SigningPrompts(
            chooseCertificate: { XCTFail("A pending Sign must not open a source"); return nil },
            askPassword: { _ in nil }, chooseIdentity: { _ in nil }))
        model.pendingHomeTool = .sign
        model.openSample()
        waitUntil({ model.toolsTab == .fillSign }, "Sign never revealed the source choices")
        XCTAssertNil(model.pendingHomeTool)
        XCTAssertEqual(model.activeShellSection, .sign)
    }

    func testToolsTabsMatchDesktopOrderAndNavigationDoesNotEdit() {
        let (client, model) = model()
        XCTAssertEqual(ToolsTab.allCases.map(\.label), ["Edit", "Annotate", "Comments", "Fill & Sign"])
        XCTAssertEqual(ToolsTab.allCases.map(\.icon), [.edit, .annotate, .comments, .sign])
        XCTAssertEqual(model.toolsTab, .annotate)
        model.setContentEditMode(true)
        model.armTextInsert(true)
        let revision = model.store.editRevision
        let page = model.store.currentPage
        for tab in ToolsTab.allCases {
            model.showsTools = false
            model.selectToolsTab(tab)
            XCTAssertTrue(model.showsTools)
            XCTAssertEqual(model.toolsTab, tab)
            model.selectToolsTab(tab)
            XCTAssertEqual(model.toolsTab, tab, "the active tab cannot be toggled off")
            XCTAssertTrue(model.store.contentEditMode)
            XCTAssertTrue(model.textInsertArmed)
        }
        XCTAssertEqual(model.store.currentPage, page)
        XCTAssertEqual(model.store.editRevision, revision)
        XCTAssertEqual(client.pageEdits, [])
    }

    func testToolsNavigationSurvivesHomeOrganizeAndFind() {
        let (_, model) = model()
        model.selectToolsTab(.comments)
        model.showsTools = false
        model.showHome()
        model.returnToDocument()
        XCTAssertEqual(model.toolsTab, .comments)
        XCTAssertFalse(model.showsTools)
        model.navigateShell(to: .organize)
        XCTAssertTrue(model.revealDocumentSearch())
        XCTAssertEqual(model.toolsTab, .comments)
        XCTAssertFalse(model.showsTools)
        model.navigateShell(to: .edit)
        XCTAssertEqual(model.toolsTab, .edit)
        XCTAssertTrue(model.showsTools)
        model.showsTools = false
        model.navigateShell(to: .annotate)
        XCTAssertEqual(model.toolsTab, .annotate)
        XCTAssertTrue(model.showsTools)
    }

    private func model(open: Bool = true, signing: SigningPrompts = .modal) -> (FakePdfCoreClient, ViewerViewModel) {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let model = ViewerViewModel(store: ViewerStore(client: client), signingPrompts: signing, sampleLoader: { Data([1]) })
        if open {
            model.openSample()
            waitUntil({ model.store.state == .loaded }, "the sample never opened")
        }
        return (client, model)
    }

    func testTheGridIsLinuxsSixToolsInItsOrder() {
        XCTAssertEqual(HomeTool.allCases.map(\.label), ["Edit", "Annotate", "Sign", "Organize", "Compress", "Protect"])
    }

    func testRailMatchesTheOtherDesktopShellsOrderAndIcons() {
        XCTAssertEqual(ShellSection.allCases.map(\.label),
            ["Home", "Recent", "My files", "Edit PDF", "Annotate", "Sign", "Organize pages", "Protect"])
        XCTAssertEqual(ShellSection.allCases.map(\.icon), [.home, .recent, .files, .edit, .annotate, .sign, .organize, .protect])
        XCTAssertEqual(ShellSection.allCases.compactMap(\.tool), [.edit, .annotate, .sign, .organize, .protect])
    }

    func testRailHomeAndRecentPreserveTheSessionAndReturnToItsMode() {
        let (_, model) = model()
        model.store.applyAnnotationEdit(.add(kind: .shape, page: 0,
            rect: PdfRect(x: 10, y: 10, width: 100, height: 100), color: .annotationDefault))
        model.isOrganizing = true
        let revision = model.store.editRevision

        model.navigateShell(to: .recent)
        XCTAssertTrue(model.isShowingHome)
        XCTAssertEqual(model.activeShellSection, .recent)
        XCTAssertEqual(model.recentFocusRequest, 1)
        model.navigateShell(to: .recent)
        XCTAssertEqual(model.recentFocusRequest, 2, "a repeated Recent click must focus the cards again")
        model.navigateShell(to: .home)
        XCTAssertEqual(model.activeShellSection, .home)
        model.returnToDocument()

        XCTAssertEqual(model.activeShellSection, .organize)
        XCTAssertNotNil(model.store.document)
        XCTAssertEqual(model.store.editRevision, revision)
        XCTAssertTrue(model.store.canUndo)
    }

    func testRailToolsLeaveOrganizeAndEditModesAsTheirDestinationsRequire() {
        let (_, model) = model()
        model.navigateShell(to: .organize)
        XCTAssertEqual(model.activeShellSection, .organize)
        model.navigateShell(to: .edit)
        XCTAssertFalse(model.isOrganizing)
        XCTAssertTrue(model.store.contentEditMode, "Edit must not be refused because Organize was showing")
        XCTAssertEqual(model.activeShellSection, .edit)
        model.navigateShell(to: .annotate)
        XCTAssertFalse(model.store.contentEditMode)
        XCTAssertNil(model.store.armedTool, "navigation must not place an unrequested annotation")
        XCTAssertEqual(model.activeShellSection, .annotate)
        model.navigateShell(to: .protect)
        XCTAssertTrue(model.isProtectPresented, "the rail uses the existing protection flow")
    }

    func testRecentAlsoWorksOnAColdStartAndBusyNavigationDoesNotChangeModes() {
        let (_, model) = model(open: false)
        model.navigateShell(to: .recent)
        XCTAssertEqual(model.activeShellSection, .recent)
        XCTAssertTrue(model.isShowingHome)
        model.isSaving = true
        model.navigateShell(to: .home)
        XCTAssertEqual(model.activeShellSection, .recent)
        XCTAssertFalse(model.canNavigateShell)
    }

    func testFindLeavesHiddenWorkspacesAndHonoursTheBusyGuard() {
        let (_, model) = model()
        model.isOrganizing = true
        model.showHome()
        XCTAssertFalse(model.showsReaderChrome)
        XCTAssertTrue(model.revealDocumentSearch())
        XCTAssertFalse(model.showsHome)
        XCTAssertFalse(model.isOrganizing)
        XCTAssertTrue(model.showsReaderChrome)

        model.isOrganizing = true
        model.isSaving = true
        XCTAssertFalse(model.revealDocumentSearch())
        XCTAssertTrue(model.isOrganizing, "a refused Find must leave the workspace alone")
        let (_, empty) = self.model(open: false)
        XCTAssertFalse(empty.revealDocumentSearch(), "Find needs an open document")
    }

    /// Windows' `ThemeParityTests` checks on `HomeLayout`, with Linux's
    /// `SIDE_COLUMN_WIDTH`.
    func testHomeStacksTheToolsColumnUnderANarrowBody() {
        XCTAssertEqual(HomeView.toolsColumnWidth, 268)
        XCTAssertTrue(HomeView.isStacked(bodyWidth: HomeView.stackBelowWidth - 1), "Home stacks its side column under a narrow body")
        XCTAssertFalse(HomeView.isStacked(bodyWidth: HomeView.stackBelowWidth), "Home keeps two columns at the breakpoint")
        XCTAssertFalse(HomeView.isStacked(bodyWidth: 1200), "Home keeps two columns in a wide window")
        XCTAssertTrue(HomeView.isStacked(bodyWidth: 0), "An unmeasured body must not claim room for two columns")
        XCTAssertEqual(HomeView.stackBelowWidth, 720)
    }

    func testHomeSearchLeavesTheTilesWhoseLabelMatchesIgnoringCase() {
        XCTAssertEqual(HomeTool.matching(""), HomeTool.allCases)
        XCTAssertEqual(HomeTool.matching("SIG"), [.sign])
        XCTAssertEqual(HomeTool.matching(" o "), [.annotate, .organize, .compress, .protect])
        XCTAssertEqual(HomeTool.matching("Retype"), [], "descriptions are not searched, as on Linux and Windows")
    }

    func testWithADocumentOpenATileGoesStraightToItsTool() {
        let (_, model) = model()

        model.openHomeTool(.organize)
        XCTAssertTrue(model.isOrganizing)
        model.isOrganizing = false

        model.openHomeTool(.edit)
        XCTAssertTrue(model.store.contentEditMode)

        model.openHomeTool(.annotate)
        XCTAssertEqual(model.store.editStatus, HomeTool.annotatePrompt)
        XCTAssertNil(model.store.armedTool, "a tile navigates; it never arms a mark for the reader")

        model.openHomeTool(.compress)
        XCTAssertTrue(model.isCompressPresented)
        model.isCompressPresented = false

        model.openHomeTool(.protect)
        XCTAssertTrue(model.isProtectPresented)
        XCTAssertNil(model.pendingHomeTool)
    }

    func testAToolPickedFromAColdStartLandsOnceItsDocumentLoads() {
        let (_, model) = model(open: false)
        model.pendingHomeTool = .organize

        model.openSample()

        waitUntil({ model.isOrganizing }, "the pending tool never landed")
        XCTAssertNil(model.pendingHomeTool)
    }

    func testAPendingToolIsDroppedWhenTheOpenFails() {
        let (_, model) = model(open: false)
        model.pendingHomeTool = .edit

        model.applyPendingHomeTool(.error(.openFailed("Not a PDF.")))

        XCTAssertNil(model.pendingHomeTool)
    }

    func testAPendingToolWaitsThroughAPasswordPrompt() {
        let (_, model) = model(open: false)
        model.pendingHomeTool = .sign

        model.applyPendingHomeTool(.error(.passwordRequired))

        XCTAssertEqual(model.pendingHomeTool, .sign)
    }

    func testHomeShowsOverTheOpenDocumentAndReturnsToItsPage() {
        let (client, model) = model()
        model.store.currentPage = 0
        let revision = model.store.editRevision

        model.showHome()
        XCTAssertTrue(model.showsHome)
        XCTAssertNotNil(model.store.document, "Home never closes the document")

        model.returnToDocument()
        XCTAssertFalse(model.showsHome)
        XCTAssertEqual(model.store.editRevision, revision)
        waitUntil({ model.store.requestedPage?.page == 0 }, "the page list was never sent back to its page")
        XCTAssertEqual(client.pageEdits, [])
    }

    func testHomeNeedsADocumentToShowOver() {
        let (_, model) = model(open: false)
        model.showHome()
        XCTAssertFalse(model.showsHome)
    }

    func testATileOnHomeOverADocumentLeavesHomeForTheTool() {
        let (_, model) = model()
        model.showHome()

        model.openHomeTool(.organize)

        XCTAssertFalse(model.showsHome)
        XCTAssertTrue(model.isOrganizing)
    }

    func testOpeningAnotherDocumentFromHomeLeavesHome() {
        let (_, model) = model()
        model.showHome()

        model.openSample()

        waitUntil({ !model.showsHome }, "Home stayed up over the new document")
    }

    func testAReleaseBuildShowsItsTagVersion() {
        XCTAssertEqual(AppVersion.label("0.1.0-beta.3"), "Vitela 0.1.0-beta.3")
        XCTAssertEqual(AppVersion.label(" 1.2.0\n"), "Vitela 1.2.0")
    }

    func testABuildWithoutAVersionSaysItIsADevelopmentOne() {
        XCTAssertEqual(AppVersion.label(nil), "Vitela dev")
        XCTAssertEqual(AppVersion.label("  "), "Vitela dev")
        XCTAssertEqual(AppVersion.label("$(VITELA_VERSION)"), "Vitela dev")
    }

    func testTheAppCarriesTheVersionKeyItsFooterReads() {
        // Tests run hosted in Vitela.app, built without VITELA_VERSION: the
        // key is there and empty, so the footer names a development build.
        XCTAssertEqual(Bundle.main.object(forInfoDictionaryKey: AppVersion.infoKey) as? String, "")
        XCTAssertEqual(AppVersion.footerText, "Vitela dev")
    }
}
