using Pdf.Windows.Viewer;

/// <summary>
/// The shell's icon contract, the same one apps/linux-gtk/src/app/icons.rs holds:
/// every icon is a shared assets/icons drawing with exactly one tint token, the
/// tint swaps that token and nothing else, and every tint role is a palette role.
/// </summary>
internal static class IconParityTests
{
    public static Task RunAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }

        foreach (var icon in Enum.GetValues<ShellIcon>())
        {
            var source = ShellIcons.Source(icon);
            Check(source.Contains("<svg"), $"{icon} must load an SVG drawing from the shared assets/icons set");
            Check(CountOf(source, ShellIcons.TintToken) == 1,
                $"{icon} must carry the tint token exactly once: none paints black, two leaves one stroke black");

            var tinted = ShellIcons.Tinted(icon, "#EC4899");
            Check(tinted.Contains("#EC4899") && !tinted.Contains(ShellIcons.TintToken), $"{icon} tint must replace the token");
            Check(CountOf(tinted, "<path") == CountOf(source, "<path"), $"{icon} tint must leave the drawing alone");
        }

        // Every drawing Linux ships is reachable from Windows; nothing in the shared set is orphaned.
        var shared = Directory.GetFiles(FindIcons(), "*.svg").Select(Path.GetFileName).Order().ToArray();
        var mapped = Enum.GetValues<ShellIcon>().Select(ShellIcons.FileName).Order().ToArray();
        Check(shared.SequenceEqual(mapped),
            $"assets/icons and ShellIcon must name the same drawings; only on disk: {string.Join(", ", shared.Except(mapped))}; only in code: {string.Join(", ", mapped.Except(shared))}");

        // Tints are palette roles, so dark mode re-tints instead of painting #51496a on #232323.
        Check(ShellIcons.PaletteKey(IconTint.Neutral) == "VitelaTextSecondaryBrush", "Neutral is the secondary text colour, as NEUTRAL_TINT is on Linux");
        Check(ShellIcons.PaletteKey(IconTint.Accent) == "VitelaAccentBrush", "Accent is the shell accent");
        Check(ShellIcons.PaletteKey(IconTint.Muted) == "VitelaMutedBrush", "Muted is the disabled label colour");
        foreach (var (tint, hex) in new[]
        {
            (IconTint.Annotate, "#14B8A6"), (IconTint.Sign, "#EC4899"), (IconTint.Organize, "#22C55E"),
            (IconTint.Compress, "#F59E0B"), (IconTint.Protect, "#6366F1"),
        })
        {
            Check(ShellIcons.PaletteKey(tint) is null, $"{tint} is a fixed tool hue, not a palette role");
            Check(ShellIcons.FixedHex(tint) == hex, $"{tint} must be {hex}, the Linux {tint.ToString().ToUpperInvariant()}_TINT");
        }
        Check(ShellIcons.PaletteKey(IconTint.Edit) == "VitelaAccentBrush", "Edit is the accent itself, as EDIT_TINT is on Linux");
        return Task.CompletedTask;
    }

    private static int CountOf(string text, string token)
    {
        var count = 0;
        for (var index = text.IndexOf(token, StringComparison.Ordinal); index >= 0; index = text.IndexOf(token, index + token.Length, StringComparison.Ordinal)) count++;
        return count;
    }

    private static string FindIcons()
    {
        for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory is not null; directory = directory.Parent)
        {
            var candidate = Path.Combine(directory.FullName, "assets", "icons");
            if (Directory.Exists(candidate) && File.Exists(Path.Combine(candidate, "home.svg"))) return candidate;
        }
        throw new DirectoryNotFoundException("assets/icons was not found above the test binaries");
    }
}
