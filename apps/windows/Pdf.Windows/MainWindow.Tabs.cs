using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Pdf.Windows.Localization;
using Pdf.Windows.Recovery;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Several documents open at once, one tab each — the "parked" model of the
/// GTK shell's <c>tabs/mod.rs</c>.
/// </summary>
/// <remarks>
/// <para>
/// Every tab owns a <see cref="PdfDocumentFacade"/> of its own, holding its
/// open document, edit history and unsaved-changes state. Only the
/// <b>active</b> tab's facade is <see cref="_facade"/> and only its session is
/// <see cref="_session"/>, so the dozens of features that read "the" document
/// keep reading those two fields unchanged. Every other tab is <i>parked</i>:
/// its facade stays open with the session, the file it came from and the
/// reading position it was left at. Its page widgets are not kept; switching
/// back rebuilds them from the open handle, with no disk read and no reparse.
/// </para>
/// <para>
/// Work started against the tab being left cannot land on the one replacing
/// it: every session id is unique, and the existing "is this still
/// <c>_session</c>?" checks drop a late result, while a late facade call with
/// the old id reaches a facade that does not hold it and is refused.
/// </para>
/// <para>
/// <b>The switching rule</b> (<see cref="TabSwitchRefusal"/>): the reader
/// cannot switch away from — or close — the active tab while an operation or a
/// dialog is acting on it. A click or Ctrl+W is refused at once with a status
/// message, never carried out later. Tool modes, toolbars, zoom and recents
/// stay window-level; recovery is per tab (<c>MainWindow.Recovery.cs</c>).
/// </para>
/// </remarks>
public sealed partial class MainWindow
{
    /// <summary>One logger for every tab's facade: the log is a per-app tail, not a per-document one.</summary>
    private static readonly FileDiagnosticLogger SharedDiagnostics = new(FileDiagnosticLogger.DefaultPath);

    private readonly DocumentTabList _tabList = new();
    private readonly Dictionary<TabId, DocumentTab> _tabs = [];

    /// <summary>Whether a switch, a close or a window-close walk is moving tabs right now.</summary>
    private bool _tabSwitching;

    /// <summary>A tab's document while it is not on screen, and what it needs to come back.</summary>
    private sealed class DocumentTab(PdfDocumentFacade facade)
    {
        public PdfDocumentFacade Facade { get; } = facade;

        /// <summary>The session as it was last seen; refreshed from the facade when the tab comes back.</summary>
        public DocumentSession? Session { get; set; }

        /// <summary>The file it was opened from or last saved to, which Save suggests.</summary>
        public string? Origin { get; set; }

        /// <summary>The reading position it was left at; <c>null</c> for a tab never shown.</summary>
        public ScrollAnchor? Anchor { get; set; }

        /// <summary>Whether it was left on the Organize screen.</summary>
        public bool Organizing { get; set; }

        /// <summary>
        /// The session-long latches a reinstall resets (<c>_pagesEdited</c>,
        /// <c>_contentEditedPages</c>, <c>_filledFieldPages</c>): an undo after
        /// coming back must still resync pages and re-render edited ones.
        /// </summary>
        public bool PagesEdited { get; set; }

        public HashSet<uint> ContentEditedPages { get; set; } = [];

        public HashSet<uint> FilledFieldPages { get; set; } = [];

        /// <summary>Whether its document has unsaved changes, as the strip last asked.</summary>
        public bool Dirty { get; set; }

        /// <summary>Its own crash-recovery entry; <c>null</c> until crash recovery is enabled.</summary>
        public AutosaveScheduler? Autosave { get; set; }
    }

    /// <summary>A facade for one tab, with the shell's conservative lifecycle policy.</summary>
    private static PdfDocumentFacade NewDocumentFacade() =>
        new(new GeneratedPdfCore(), SharedDiagnostics, conservativeLifecycle: true);

    private DocumentTab? ActiveTab => _tabList.Active is { } id && _tabs.TryGetValue(id, out var tab) ? tab : null;

    private DocumentTab NewTab(PdfDocumentFacade facade)
    {
        var tab = new DocumentTab(facade);
        tab.Autosave = NewTabAutosave(tab);
        return tab;
    }

    /// <summary>
    /// A document has just been installed on screen. A document shown with no
    /// active tab — the first one, or one a harness opened directly into the
    /// window's facade — gets a tab of its own; otherwise the active tab
    /// records what it now holds, since a reopen (Protect, Sign, a reload)
    /// replaces the document inside the same tab.
    /// </summary>
    private void ActiveTabSessionInstalled()
    {
        if (_tabList.Active is not { } id || !_tabs.ContainsKey(id))
        {
            id = _tabList.Add(_documentOrigin);
            _tabs[id] = NewTab(_facade);
            _tabList.Activate(id);
        }
        var tab = _tabs[id];
        tab.Session = _session;
        tab.Origin = _documentOrigin;
        _tabList.SetPath(id, _documentOrigin);
        SyncTabStrip();
    }

    /// <summary>The active tab now lives at <paramref name="path"/> — a Save As moved it.</summary>
    private void ActiveTabMoved(string? path)
    {
        if (_tabList.Active is not { } id || !_tabs.TryGetValue(id, out var tab)) return;
        tab.Origin = path;
        _tabList.SetPath(id, path);
        SyncTabStrip();
    }

    /// <summary>
    /// Whether <paramref name="path"/> is held by a tab other than the active
    /// one; reports the refusal when it is. Every write that adopts its
    /// destination as the document's file checks it, so two tabs never claim
    /// one file.
    /// </summary>
    private bool RefuseHeldDestination(string? path)
    {
        if (!_tabList.HeldByAnotherTab(path, _tabList.Active)) return false;
        AnnotationStatus.Text = I18n.T("tabs-path-open-elsewhere");
        return true;
    }

    /// <summary>
    /// Why the active tab cannot be switched away from or closed right now,
    /// or <c>null</c> when it can.
    /// </summary>
    private string? TabSwitchRefusal()
    {
        if (_dialogOpen || IsPaletteOpen) return I18n.T("tabs-dialog-open");
        if (_isBusy || _shellPickingFile || _tabSwitching || _tabsOpening || _confirmingClose
            || _savingDocument || _protectingDocument || _compressingDocument || _exportingImages
            || _extractingPages || _splittingPages || _printingDocument || _sharingDocument
            || _signingFlowActive || _organizeBusy || _addingWatermark) return I18n.T("tabs-busy");
        return null;
    }
}
