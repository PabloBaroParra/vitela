using System.Reflection;

namespace Pdf.Windows.Viewer;

/// <summary>
/// One drawing from the shared assets/icons set — the same files the GTK shell
/// draws through apps/linux-gtk/src/app/icons.rs. It also includes the Windows
/// navigation and Android annotation drawings, authored on the same grid.
/// </summary>
public enum ShellIcon
{
    Home, Recent, Files, Edit, Annotate, Sign, Organize, Compress, Protect,
    NewFile, Sample, Delete, Text, Image, Save, Print, Share, ExportImages, Undo, Redo,
    ZoomOut, ZoomIn, FitWidth, FitPage, PanelLeft, PanelRight, Previous, Next,
    Search, Comments, MoveUp, MoveDown, RotateLeft, RotateRight,
    FirstPage, LastPage, GoToPage, ExtractPages, SplitPages,
    Select, Highlight, Underline, Strikeout, Ink, Shape,
}

/// <summary>
/// The colour an icon is drawn in, by role. Linux bakes light-theme hex values;
/// here the neutral, accent and muted roles resolve through the palette so a dark
/// window re-tints them, while the per-tool hues stay fixed as on Linux.
/// </summary>
public enum IconTint { Neutral, Accent, Muted, Edit, Annotate, Sign, Organize, Compress, Protect }

/// <summary>
/// Loads and tints the shared icon drawings. Neither SvgImageSource nor librsvg
/// resolves currentColor, so each file carries <see cref="TintToken"/> exactly once
/// on the group that owns its strokes, and the caller's colour replaces it.
/// </summary>
public static class ShellIcons
{
    /// <summary>Black, so a file opened outside the app still shows its drawing.</summary>
    public const string TintToken = "#000000";

    private static readonly Dictionary<ShellIcon, string> Sources = [];

    public static string FileName(ShellIcon icon) => icon switch
    {
        ShellIcon.NewFile => "new-file.svg",
        ShellIcon.ExportImages => "export-images.svg",
        ShellIcon.ZoomOut => "zoom-out.svg",
        ShellIcon.ZoomIn => "zoom-in.svg",
        ShellIcon.FitWidth => "fit-width.svg",
        ShellIcon.FitPage => "fit-page.svg",
        ShellIcon.PanelLeft => "panel-left.svg",
        ShellIcon.PanelRight => "panel-right.svg",
        ShellIcon.MoveUp => "move-up.svg",
        ShellIcon.MoveDown => "move-down.svg",
        ShellIcon.RotateLeft => "rotate-left.svg",
        ShellIcon.RotateRight => "rotate-right.svg",
        ShellIcon.FirstPage => "first-page.svg",
        ShellIcon.LastPage => "last-page.svg",
        ShellIcon.GoToPage => "go-to-page.svg",
        ShellIcon.ExtractPages => "extract-pages.svg",
        ShellIcon.SplitPages => "split-pages.svg",
        _ => icon.ToString().ToLowerInvariant() + ".svg",
    };

    /// <summary>
    /// The authored SVG, embedded at build time like Linux's include_str!, so a
    /// missing file is a build error rather than a blank button.
    /// </summary>
    public static string Source(ShellIcon icon)
    {
        lock (Sources)
        {
            if (Sources.TryGetValue(icon, out var cached)) return cached;
            var name = "Pdf.Windows.Icons." + FileName(icon);
            using var stream = Assembly.GetExecutingAssembly().GetManifestResourceStream(name)
                ?? throw new InvalidOperationException($"Icon resource {name} is not embedded");
            using var reader = new StreamReader(stream);
            return Sources[icon] = reader.ReadToEnd();
        }
    }

    public static string Tinted(ShellIcon icon, string hex) => Source(icon).Replace(TintToken, hex, StringComparison.Ordinal);

    /// <summary>The palette brush a role follows, or null for a fixed tool hue.</summary>
    public static string? PaletteKey(IconTint tint) => tint switch
    {
        IconTint.Neutral => "VitelaTextSecondaryBrush",
        IconTint.Accent or IconTint.Edit => "VitelaAccentBrush",
        IconTint.Muted => "VitelaMutedBrush",
        _ => null,
    };

    /// <summary>The per-tool hue from the reference design, for roles with no palette key.</summary>
    public static string? FixedHex(IconTint tint) => tint switch
    {
        IconTint.Annotate => "#14B8A6",
        IconTint.Sign => "#EC4899",
        IconTint.Organize => "#22C55E",
        IconTint.Compress => "#F59E0B",
        IconTint.Protect => "#6366F1",
        _ => null,
    };
}
