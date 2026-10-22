import Foundation

/// Home's footer: which build of Vitela is running — Linux's
/// `home/version.rs` and Windows' `Viewer/AppVersion.cs`. The release tag is
/// the only source of a real version, and `MARKETING_VERSION` never follows
/// it (it would read 0.1.0 on every beta). So the version is whatever the
/// build was handed in `VITELA_VERSION`, which `scripts/build-macos.sh`
/// passes to Xcode and `Info.plist` stores as `VitelaVersion`; any other
/// build says it is a development one instead of claiming a number.
enum AppVersion {
    static let infoKey = "VitelaVersion"

    static var footerText: String {
        label(Bundle.main.object(forInfoDictionaryKey: infoKey) as? String)
    }

    /// The footer's text for a build `version`; an unset or blank one is
    /// `dev`. An unexpanded `$(VITELA_VERSION)` is unset too.
    static func label(_ version: String?) -> String {
        let name = version?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let known = !name.isEmpty && !name.hasPrefix("$(")
        return "Vitela \(known ? name : "dev")"
    }
}
