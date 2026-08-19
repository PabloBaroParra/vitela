using Pdf.Windows.Facade;

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

    private sealed class PageContentState
    {
        public PageContent? Content { get; set; }
        public Task<PageContent?>? Loading { get; set; }
        public bool Denied { get; set; }
    }
}
