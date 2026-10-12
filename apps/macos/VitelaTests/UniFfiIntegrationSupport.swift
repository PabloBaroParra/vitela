import XCTest
@testable import Vitela

/// What every real-core integration test shares. The test host is the app
/// bundle, so its `Contents/Frameworks` carries both dylibs.
///
/// Each test body runs on a background-QoS queue while the main thread waits
/// on an expectation (which spins the run loop rather than blocking). Every
/// core call waits on PDFium's render thread; waiting on it from a thread of
/// higher QoS trips Xcode's Thread Performance Checker, and symbolicating its
/// report has been seen to hang the whole test run. The lowest QoS cannot be
/// inverted. The stores have no observers here, so leaving the main thread
/// changes nothing they do.
extension XCTestCase {
    func offMain(_ body: @escaping () throws -> Void) throws {
        let done = expectation(description: "test body finished")
        var failure: Error?
        DispatchQueue.global(qos: .background).async {
            do { try body() } catch { failure = error }
            done.fulfill()
        }
        wait(for: [done], timeout: 30)
        if let failure { throw failure }
    }

    static func sampleBytes() throws -> Data {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "vitela-sample", withExtension: "pdf"))
        return try Data(contentsOf: url)
    }
}
