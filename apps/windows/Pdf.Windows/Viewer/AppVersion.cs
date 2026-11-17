using System.Reflection;
using Pdf.Windows.Localization;

namespace Pdf.Windows.Viewer;

/// <summary>
/// Home's footer: which build of Vitela is running. The release tag is the
/// only source of a real version; the csproj stamps its semver into the
/// assembly's informational version (-p:VitelaVersion), and any other build
/// says "dev" (localized, as on Linux). The MSIX package version is not
/// used: 0.1.103.0 is the Store's encoding of 0.1.0-beta.3, not a name
/// anyone would recognise.
/// </summary>
public static class AppVersion
{
    public static string FooterText { get; } =
        Label(typeof(AppVersion).Assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion);

    /// <summary>The footer's text for an informational version; an unset, blank or "dev" one is a development build.</summary>
    public static string Label(string? version)
    {
        // Defensive: the SDK appends "+<commit>" unless the csproj turns it off.
        var name = version?.Split('+')[0].Trim();
        // The csproj stamps a non-release build as "dev"; that marker is shown localized, as on Linux.
        var development = string.IsNullOrEmpty(name) || name == "dev";
        return I18n.T("home-version", ("version", development ? I18n.T("home-version-dev") : name!));
    }
}
