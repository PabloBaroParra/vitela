using System.Xml.Linq;
using Pdf.Windows.Viewer;

/// <summary>
/// The shell's theme contract: one palette file, the same roles in both
/// variants, the user-approved values, and Home's two-column breakpoint.
/// </summary>
internal static class ThemeParityTests
{
    private static readonly XNamespace Xaml = "http://schemas.microsoft.com/winfx/2006/xaml";
    private static readonly XNamespace Presentation = "http://schemas.microsoft.com/winfx/2006/xaml/presentation";

    public static Task RunAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }

        var light = Variant("Light");
        var dark = Variant("Dark");
        Check(light.Keys.Order().SequenceEqual(dark.Keys.Order()),
            "Light and Dark must define exactly the same keys; a role missing from one variant falls back to a WinUI default colour");
        Check(light.Count > 20, "The palette must define the full set of roles, not a stub");

        foreach (var (key, lightValue, darkValue) in new[]
        {
            ("VitelaWindowBackgroundBrush", "#F8F7FB", "#232323"),
            ("VitelaSurfaceBrush", "#FFFFFF", "#282828"),
            ("VitelaInputBackgroundBrush", "#FCFBFE", "#2E2E2E"),
            ("VitelaEditorBackgroundBrush", "#F2F0F5", "#212121"),
            ("VitelaCanvasBackgroundBrush", "#E9E6EC", "#1C1C1C"),
            ("VitelaHairlineBrush", "#E3E0E9", "#333333"),
            ("VitelaInputBorderBrush", "#DED9E9", "#3F3F3F"),
            ("VitelaStrongSeparatorBrush", "#C9C2E0", "#555555"),
            ("VitelaHoverBrush", "#F5F3FA", "#333333"),
            ("VitelaTextPrimaryBrush", "#302D3A", "#DADADA"),
            ("VitelaTextSecondaryBrush", "#51496A", "#B3B3B3"),
            ("VitelaTextHintBrush", "#625B72", "#999999"),
            ("VitelaMutedBrush", "#A49FB3", "#666666"),
            ("VitelaAccentBrush", "#6B4EFF", "#A882FF"),
            ("VitelaAccentFillBrush", "#6B4EFF", "#6B4EFF"),
            ("VitelaAccentFillHoverBrush", "#5A3EE6", "#5A3EE6"),
            ("VitelaAccentSoftBrush", "#EEE9FA", "#373144"),
            ("VitelaCheckedBrush", "#F2EDFF", "#3D364F"),
            ("VitelaAccentBorderBrush", "#DED3FF", "#58497B"),
            ("VitelaSuccessBrush", "#1F8A4C", "#44CF6E"),
            ("VitelaDisabledFillBrush", "#F5F4F7", "#282828"),
            ("VitelaDisabledBorderBrush", "#EAE8EF", "#333333"),
            ("VitelaCoverSheetBrush", "#F1EEF6", "#262626"),
        })
        {
            Check(light.GetValueOrDefault(key) == lightValue, $"{key} light must be {lightValue}, was {light.GetValueOrDefault(key)}");
            Check(dark.GetValueOrDefault(key) == darkValue, $"{key} dark must be {darkValue}, was {dark.GetValueOrDefault(key)}");
        }

        Check(HomeLayout.IsStacked(HomeLayout.StackBelowWidth - 1), "Home stacks its side column under a narrow body");
        Check(!HomeLayout.IsStacked(HomeLayout.StackBelowWidth), "Home keeps two columns at the breakpoint");
        Check(!HomeLayout.IsStacked(1200), "Home keeps two columns in a wide window");
        Check(HomeLayout.IsStacked(0), "An unmeasured body must not claim to have room for two columns");

        // The caption is drawn by the OS from AppWindow.TitleBar colours, which cannot
        // reference XAML resources, so TitleBarColors repeats four palette roles.
        foreach (var (isDark, variant) in new[] { (false, light), (true, dark) })
        {
            var colors = TitleBarColors.For(isDark);
            var theme = isDark ? "dark" : "light";
            Check(Hex(colors.Background) == variant["VitelaSurfaceBrush"], $"title bar background ({theme}) must be the surface colour");
            Check(Hex(colors.Foreground) == variant["VitelaTextPrimaryBrush"], $"title bar text ({theme}) must be the primary text colour");
            Check(Hex(colors.HoverBackground) == variant["VitelaHoverBrush"], $"caption button hover ({theme}) must be the hover colour");
            Check(Hex(colors.PressedBackground) == variant["VitelaAccentSoftBrush"], $"caption button pressed ({theme}) must be accent-soft");
            Check(Hex(colors.InactiveForeground) == variant["VitelaMutedBrush"], $"inactive caption text ({theme}) must be the muted colour");
        }
        return Task.CompletedTask;
    }

    private static string Hex(uint rgb) => $"#{rgb:X6}";

    /// <summary>The colour of every resource in one ThemeDictionaries variant, upper-cased #RRGGBB.</summary>
    private static Dictionary<string, string> Variant(string name)
    {
        var path = FindPalette();
        var document = XDocument.Load(path);
        var variant = document.Descendants()
            .Where(element => (string?)element.Attribute(Xaml + "Key") == name && element.Parent?.Name.LocalName == "ResourceDictionary.ThemeDictionaries")
            .SingleOrDefault() ?? throw new InvalidOperationException($"{path} has no {name} theme dictionary");
        var result = new Dictionary<string, string>();
        foreach (var element in variant.Elements())
        {
            var key = (string?)element.Attribute(Xaml + "Key");
            if (key is null) continue;
            var color = (string?)element.Attribute("Color") ?? element.Value.Trim();
            result[key] = color.Length == 9 ? "#" + color[3..].ToUpperInvariant() : color.ToUpperInvariant();
        }
        return result;
    }

    private static string FindPalette()
    {
        for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory is not null; directory = directory.Parent)
        {
            var candidate = Path.Combine(directory.FullName, "apps", "windows", "Pdf.Windows", "Themes", "Palette.xaml");
            if (File.Exists(candidate)) return candidate;
        }
        throw new FileNotFoundException("apps/windows/Pdf.Windows/Themes/Palette.xaml was not found above the test binaries");
    }
}
