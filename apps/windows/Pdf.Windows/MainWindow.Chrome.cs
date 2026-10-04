using System.Runtime.InteropServices;
using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Pdf.Windows.Viewer;
using WinRT.Interop;
using WinColor = global::Windows.UI.Color;

namespace Pdf.Windows;

/// <summary>Window chrome: the app icon and a title bar that follows the system theme.</summary>
public sealed partial class MainWindow
{
    /// <summary>Next to the exe; the project copies it there (Pdf.Windows.csproj).</summary>
    private static readonly string IconPath = Path.Combine(AppContext.BaseDirectory, "Assets", "vitela.ico");

    private void InitializeWindowChrome()
    {
        // The exe resource (ApplicationIcon) covers Explorer and a pinned taskbar
        // button; the running window, title bar and Alt-Tab need it set explicitly.
        try { if (File.Exists(IconPath)) AppWindow.SetIcon(IconPath); }
        catch { /* A missing icon must never stop the window opening. */ }

        ApplyTitleBarTheme();
        // ActualTheme is only real once the root is in the tree.
        ShellRoot.Loaded += (_, _) => ApplyTitleBarTheme();
        ShellRoot.ActualThemeChanged += (_, _) => ApplyTitleBarTheme();
    }

    private void ApplyTitleBarTheme()
    {
        var dark = ShellRoot.ActualTheme == ElementTheme.Dark;
        // AppWindow.TitleBar only paints the caption; DWM still draws the frame and
        // the 1px separator under it in the light theme unless told otherwise.
        UseImmersiveDarkMode(dark);
        if (!AppWindowTitleBar.IsCustomizationSupported()) return;
        var colors = TitleBarColors.For(dark);
        var titleBar = AppWindow.TitleBar;
        titleBar.BackgroundColor = titleBar.InactiveBackgroundColor = Rgb(colors.Background);
        titleBar.ForegroundColor = Rgb(colors.Foreground);
        titleBar.InactiveForegroundColor = Rgb(colors.InactiveForeground);
        titleBar.ButtonBackgroundColor = titleBar.ButtonInactiveBackgroundColor = Rgb(colors.Background);
        titleBar.ButtonForegroundColor = titleBar.ButtonHoverForegroundColor = titleBar.ButtonPressedForegroundColor = Rgb(colors.Foreground);
        titleBar.ButtonInactiveForegroundColor = Rgb(colors.InactiveForeground);
        titleBar.ButtonHoverBackgroundColor = Rgb(colors.HoverBackground);
        titleBar.ButtonPressedBackgroundColor = Rgb(colors.PressedBackground);
    }

    private const int DwmwaUseImmersiveDarkMode = 20;

    [DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

    private void UseImmersiveDarkMode(bool dark)
    {
        var enabled = dark ? 1 : 0;
        // Best effort: an older build without the attribute keeps its light frame.
        _ = DwmSetWindowAttribute(WindowNative.GetWindowHandle(this), DwmwaUseImmersiveDarkMode, ref enabled, sizeof(int));
    }

    private static WinColor Rgb(uint rgb) => WinColor.FromArgb(255, (byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb);
}
