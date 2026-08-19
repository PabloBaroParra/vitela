using Microsoft.UI.Text;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.UI.Text;

namespace Pdf.Windows;

/// <summary>
/// The parsed content of the pages on screen: what content-edit mode reads to
/// know where the text runs are.
///
/// The core never caches this — page content is parsed on demand, unlike
/// annotations — so the cache lives here, one entry per page, for as long as
/// the document is open. It survives a preview refresh, which leaves the bytes
/// it was parsed from untouched, and dies with the document, whose bytes are
/// the only thing its run ids mean anything against.
///
/// Mirrors the GTK shell's <c>content_edit::model</c>.
/// </summary>
public sealed partial class MainWindow
{
    /// <summary>Page content, parsed once per page and kept for the session's lifetime.</summary>
    private readonly Dictionary<uint, PageContentState> _pageContent = [];

    /// <summary>
    /// Loads and caches one page's content. A refusal is cached too, so a
    /// document that withholds content editing does not re-ask the facade on
    /// every click.
    /// </summary>
    private async Task<PageContent?> EnsurePageContentAsync(uint pageIndex)
    {
        if (_session is null)
        {
            return null;
        }

        if (!_pageContent.TryGetValue(pageIndex, out var state))
        {
            state = new PageContentState();
            _pageContent[pageIndex] = state;
        }

        if (state.Content is not null || state.Denied)
        {
            return state.Content;
        }

        if (state.Loading is { } inFlight)
        {
            return await inFlight;
        }

        var sessionId = _session.SessionId;
        var load = LoadPageContentAsync(sessionId, pageIndex, state);
        state.Loading = load;
        try
        {
            return await load;
        }
        finally
        {
            state.Loading = null;
        }
    }

    private async Task<PageContent?> LoadPageContentAsync(string sessionId, uint pageIndex, PageContentState state)
    {
        var result = await _facade.PageContentAsync(sessionId, pageIndex);
        if (_session is null || _session.SessionId != sessionId)
        {
            // A different document opened while this was in flight — the cache
            // it would populate has already been cleared.
            return null;
        }

        if (!result.IsSuccess)
        {
            state.Denied = true;
            AnnotationStatus.Text = result.Error!.Message;
            return null;
        }

        state.Content = result.Value;
        DrawContentOutlines(pageIndex);
        return state.Content;
    }

    /// <summary>
    /// Where a retyped run sits now, keyed by page and run id.
    /// </summary>
    /// <remarks>
    /// The parsed content describes the document as opened and never moves —
    /// its boxes are what the core matches an edit against, so they must not.
    /// But the page shows the replacement, which is a different length, and
    /// the outline and the hit-test have to describe what the reader can see
    /// and click, not what was parsed.
    /// </remarks>
    private readonly Dictionary<(uint Page, ulong Run), AnnotationRect> _pendingRunBounds = [];

    /// <summary>The box a run occupies on the page right now.</summary>
    private AnnotationRect BoundsOf(ContentTextRun run) =>
        _pendingRunBounds.TryGetValue((run.PageIndex, run.Id), out var pending) ? pending : run.Bounds;

    /// <summary>
    /// Records how much room a run takes after being retyped.
    /// </summary>
    /// <remarks>
    /// Measured as a ratio, not as an absolute width: the same string is
    /// measured twice in the same stand-in face, so what survives the
    /// comparison is how much longer or shorter the new text is — which
    /// carries over to the document's own font far better than either
    /// measurement does on its own.
    ///
    /// An estimate, and it says so: the page itself is the authority, and it
    /// has already been re-rendered by the time anyone looks. This is only how
    /// the outline and the hit-test keep up between renders.
    /// </remarks>
    private void RecordPendingBounds(ContentTextRun run, string text)
    {
        var was = MeasureWidth(run, run.Text);
        var now = MeasureWidth(run, text);
        var width = was > 0 ? run.Bounds.Width * (now / was) : run.Bounds.Width;
        _pendingRunBounds[(run.PageIndex, run.Id)] =
            new AnnotationRect(run.Bounds.X, run.Bounds.Y, width, run.Bounds.Height);
    }

    /// <summary>
    /// How wide <paramref name="text"/> runs in the face that stands in for
    /// the one <paramref name="run"/> is painted with.
    /// </summary>
    private static double MeasureWidth(ContentTextRun run, string text)
    {
        if (text.Length == 0)
        {
            return 0;
        }

        var (families, bold, italic) = PdfFontMatch.ForBaseFont(run.BaseFont);
        var probe = new TextBlock
        {
            Text = text,
            FontFamily = new FontFamily(families),
            FontWeight = bold ? FontWeights.Bold : FontWeights.Normal,
            FontStyle = italic ? FontStyle.Italic : FontStyle.Normal,
            FontSize = Math.Max(1, run.Bounds.Height),
        };
        probe.Measure(new global::Windows.Foundation.Size(double.PositiveInfinity, double.PositiveInfinity));
        return probe.DesiredSize.Width;
    }

    private sealed class PageContentState
    {
        public PageContent? Content { get; set; }
        public Task<PageContent?>? Loading { get; set; }
        public bool Denied { get; set; }
    }
}
