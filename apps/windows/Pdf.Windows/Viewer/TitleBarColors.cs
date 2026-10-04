namespace Pdf.Windows.Viewer;

/// <summary>
/// The caption colours for one theme, as 0xRRGGBB.
/// </summary>
/// <remarks>
/// The OS paints the title bar from <c>AppWindow.TitleBar</c>, which takes plain
/// colours and cannot reference XAML resources, so these repeat five roles of
/// Themes/Palette.xaml (surface, primary text, hover, accent-soft, muted).
/// <c>ThemeParityTests</c> compares them with the palette, so they cannot drift.
/// </remarks>
public readonly record struct TitleBarColors(
    uint Background, uint Foreground, uint HoverBackground, uint PressedBackground, uint InactiveForeground)
{
    public static TitleBarColors For(bool dark) => dark
        ? new TitleBarColors(0x282828, 0xDADADA, 0x333333, 0x373144, 0x666666)
        : new TitleBarColors(0xFFFFFF, 0x302D3A, 0xF5F3FA, 0xEEE9FA, 0xA49FB3);
}
