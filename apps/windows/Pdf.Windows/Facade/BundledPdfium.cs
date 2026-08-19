namespace Pdf.Windows.Facade;

/// <summary>
/// Points the Rust core at the PDFium copy the package ships beside the
/// executable.
/// </summary>
/// <remarks>
/// <para>
/// PDFium is a prebuilt platform binary, never vendored into the Rust source
/// tree, and <c>pdf-render</c> resolves it at runtime in three steps
/// (<c>core/pdf-render/src/library.rs</c>): the <c>PDFIUM_DYNAMIC_LIB_PATH</c>
/// override, then a <em>compile-time</em> path into the developer's
/// <c>vendor/pdfium</c> tree, then the bare library name through the OS loader.
/// </para>
/// <para>
/// Only the first step describes a shipped app. The second resolves against the
/// build machine's own checkout, so it silently works during development and
/// cannot work anywhere else; the third would leave the loader free to bind
/// whatever <c>pdfium.dll</c> the search path happens to offer. The package
/// therefore names its own copy explicitly — the same thing the Linux launcher
/// script does for the .deb/.AppImage.
/// </para>
/// <para>
/// An override already present in the environment wins: it is how CI and the
/// core's own tests aim the loader, and an app that overwrote it would make
/// those runs untestable.
/// </para>
/// </remarks>
internal static class BundledPdfium
{
    /// <summary>File name the package stages next to the executable.</summary>
    internal const string LibraryFileName = "pdfium.dll";

    /// <summary>Environment variable <c>pdf-render</c> reads first.</summary>
    internal const string PathVariable = "PDFIUM_DYNAMIC_LIB_PATH";

    /// <summary>
    /// Decides the value <see cref="PathVariable"/> should take, or
    /// <see langword="null"/> to leave the environment untouched.
    /// </summary>
    /// <param name="currentOverride">Override already in the environment, if any.</param>
    /// <param name="baseDirectory">Directory the running binary loads from.</param>
    /// <param name="fileExists">Existence probe, injected so this stays testable.</param>
    internal static string? ResolveOverride(string? currentOverride, string baseDirectory, Func<string, bool> fileExists)
    {
        if (!string.IsNullOrWhiteSpace(currentOverride))
        {
            return null;
        }

        var bundled = Path.Combine(baseDirectory, LibraryFileName);
        return fileExists(bundled) ? bundled : null;
    }

    /// <summary>
    /// Applies <see cref="ResolveOverride"/> to the running process. Call it
    /// before the first core operation — the renderer reads the variable when
    /// it loads PDFium, which happens on the first document opened.
    /// </summary>
    internal static void PointCoreAtBundledLibrary()
    {
        var resolved = ResolveOverride(
            Environment.GetEnvironmentVariable(PathVariable),
            AppContext.BaseDirectory,
            File.Exists);

        if (resolved is not null)
        {
            Environment.SetEnvironmentVariable(PathVariable, resolved);
        }
    }
}
