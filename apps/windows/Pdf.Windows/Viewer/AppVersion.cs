using System.Reflection;

namespace Pdf.Windows.Viewer;

/// <summary>
/// Home's footer: which build of Vitela is running. The release tag is the
/// only source of a real version; the csproj stamps its semver into the
/// assembly's informational version (-p:VitelaVersion), and any other build
/// says "dev". The MSIX package version is not used: 0.1.103.0 is the Store's
/// encoding of 0.1.0-beta.3, not a name anyone would recognise.
/// </summary>
public static class AppVersion
{
    public static string FooterText { get; } =
        Label(typeof(AppVersion).Assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion);

    /// <summary>The footer's text for an informational version; an unset or blank one is "dev".</summary>
    public static string Label(string? version)
    {
        // Defensive: the SDK appends "+<commit>" unless the csproj turns it off.
        var name = version?.Split('+')[0].Trim();
        return $"Vitela {(string.IsNullOrEmpty(name) ? "dev" : name)}";
    }
}
