namespace Pdf.Windows.Viewer;

/// <summary>
/// The commands run from the palette this session (B27 decision 8), a port of
/// the GTK shell's <c>palette/recent.rs</c>. Kept in memory for as long as
/// the window lives; deliberately not saved to disk.
/// </summary>
internal sealed class PaletteRecents
{
    /// <summary>Entry ids, most recently run first, each at most once.</summary>
    private readonly List<string> _ids = [];

    /// <summary>Records that <paramref name="id"/> just ran: it moves to the front.</summary>
    public void Record(string id)
    {
        _ids.Remove(id);
        _ids.Insert(0, id);
    }

    /// <summary>
    /// <paramref name="items"/> with the recently run ones first, most recent
    /// first; the rest keep the order they came in.
    /// </summary>
    public IReadOnlyList<T> Order<T>(IEnumerable<T> items, Func<T, string> id) =>
        // OrderBy is stable: everything not recent shares one key and keeps its order.
        items.OrderBy(item => _ids.IndexOf(id(item)) is var position and >= 0 ? position : int.MaxValue).ToList();
}
