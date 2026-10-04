using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Storage.Streams;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Draws the shared assets/icons set, tinted per role, instead of Segoe glyphs —
/// the Windows half of apps/linux-gtk/src/app/icons.rs.
/// </summary>
public sealed partial class MainWindow
{
    /// <summary>
    /// One SvgImageSource per drawing and colour. A source is immutable once set and
    /// XAML lets many Images share it, so four hundred Organize delete buttons cost
    /// one parse — the same reason Linux caches its textures.
    /// </summary>
    private static readonly Dictionary<(ShellIcon, string), SvgImageSource> IconSources = [];

    /// <summary>
    /// A decorative icon <paramref name="size"/> px square. It follows the theme and,
    /// when <paramref name="owner"/> is disabled, the muted colour — an Image has no
    /// Foreground for the button's disabled state to dim, unlike the FontIcon it replaces.
    /// </summary>
    private static Image ShellIconImage(ShellIcon icon, double size, IconTint tint, Control? owner = null)
    {
        var image = new Image
        {
            Width = size, Height = size, Stretch = Stretch.Uniform, IsHitTestVisible = false,
            VerticalAlignment = VerticalAlignment.Center, HorizontalAlignment = HorizontalAlignment.Center,
        };
        // Callers name the control; announcing the picture too would repeat that name.
        AutomationProperties.SetAccessibilityView(image, AccessibilityView.Raw);
        void Draw() => image.Source = IconSource(icon, IconHex(owner is { IsEnabled: false } ? IconTint.Muted : tint, image.ActualTheme));
        Draw();
        image.ActualThemeChanged += (_, _) => Draw();
        if (owner is not null) owner.IsEnabledChanged += (_, _) => Draw();
        return image;
    }

    private static SvgImageSource IconSource(ShellIcon icon, string hex)
    {
        if (IconSources.TryGetValue((icon, hex), out var cached)) return cached;
        var source = new SvgImageSource();
        IconSources[(icon, hex)] = source;
        _ = LoadIconAsync(source, ShellIcons.Tinted(icon, hex));
        return source;
    }

    private static async Task LoadIconAsync(SvgImageSource source, string svg)
    {
        using var stream = new InMemoryRandomAccessStream();
        await stream.WriteAsync(Encoding.UTF8.GetBytes(svg).AsBuffer());
        stream.Seek(0);
        var status = await source.SetSourceAsync(stream);
        // The label still names the control; IconParityTests is what keeps a drawing parseable.
        if (status != SvgImageSourceLoadStatus.Success) System.Diagnostics.Debug.WriteLine($"Icon failed to load: {status}");
    }

    /// <summary>
    /// The role's colour in <paramref name="theme"/>, read from that variant's
    /// ThemeDictionary: an element's ActualTheme, not the app's, decides what it sits on.
    /// </summary>
    private static string IconHex(IconTint tint, ElementTheme theme)
    {
        if (ShellIcons.FixedHex(tint) is { } fixedHex) return fixedHex;
        var key = ShellIcons.PaletteKey(tint)!;
        var variant = theme == ElementTheme.Dark ? "Dark" : "Light";
        var brush = FindThemeBrush(Application.Current.Resources, variant, key)
            ?? (SolidColorBrush)Application.Current.Resources[key];
        var color = brush.Color;
        return $"#{color.R:X2}{color.G:X2}{color.B:X2}";
    }

    private static SolidColorBrush? FindThemeBrush(ResourceDictionary dictionary, string variant, string key)
    {
        if (dictionary.ThemeDictionaries.TryGetValue(variant, out var found)
            && found is ResourceDictionary themed && themed.TryGetValue(key, out var value) && value is SolidColorBrush brush)
            return brush;
        foreach (var merged in dictionary.MergedDictionaries)
            if (FindThemeBrush(merged, variant, key) is { } nested) return nested;
        return null;
    }
}
