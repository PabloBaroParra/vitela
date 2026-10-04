namespace Pdf.Windows.Viewer;

/// <summary>
/// Home's one responsive rule: the tools column sits beside the welcome and
/// recent blocks while the body has room for both, and drops under them
/// otherwise. Linux gets the same result by letting the body scroll sideways;
/// WinUI cannot (a horizontally scrolling parent measures its children at
/// infinite width, which is exactly what broke the first Windows Home), so the
/// column reflows instead.
/// </summary>
public static class HomeLayout
{
    /// <summary>Width of the tools column, as in Linux's SIDE_COLUMN_WIDTH.</summary>
    public const double SideColumnWidth = 268;

    /// <summary>Body width below which the side column stacks under the main one.</summary>
    public const double StackBelowWidth = 720;

    public static bool IsStacked(double bodyWidth) => !(bodyWidth >= StackBelowWidth);
}
