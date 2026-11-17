using Pdf.Windows.Localization;

namespace Pdf.Windows.Viewer;

/// <summary>Steps through search hits, wrapping at either end.</summary>
public static class SearchSelection
{
    public static string Status(string query, int selectedIndex, int count) =>
        I18n.T("search-match", ("index", selectedIndex + 1), ("total", count), ("query", query));

    public static int StepIndex(int selectedIndex, int count, int delta)
    {
        if (count == 0) return -1;
        if (selectedIndex < 0 || selectedIndex >= count) return delta < 0 ? count - 1 : 0;
        return (selectedIndex + count + delta) % count;
    }
}
