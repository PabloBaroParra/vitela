using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;
using Windows.Storage;

namespace Pdf.Windows;

/// <summary>Recent-file cards and preview lifetime, independent of the open editor session.</summary>
public sealed partial class MainWindow
{
    private ulong _recentGeneration;
    private bool _recentsClosed;
    private FileSystemWatcher? _recentWatcher;
    private readonly DispatcherTimer _recentRefreshTimer = new() { Interval = TimeSpan.FromMilliseconds(250) };
    private readonly List<(StackPanel Panel, List<Button> Cards)> _recentGroups = [];

    private void InitializeRecents()
    {
        _recentRefreshTimer.Tick += (_, _) =>
        {
            _recentRefreshTimer.Stop();
            _ = RefreshRecentFilesAsync();
        };
        try
        {
            if (Directory.Exists(WindowsRecentDocuments.DirectoryPath))
            {
                _recentWatcher = new FileSystemWatcher(WindowsRecentDocuments.DirectoryPath, "*.lnk");
                _recentWatcher.Created += RecentStore_Changed;
                _recentWatcher.Changed += RecentStore_Changed;
                _recentWatcher.Deleted += RecentStore_Changed;
                _recentWatcher.Renamed += RecentStore_Changed;
                _recentWatcher.EnableRaisingEvents = true;
            }
        }
        catch { /* Home still refreshes on each visit if notifications are unavailable. */ }
        Closed += (_, _) =>
        {
            _recentsClosed = true;
            _recentGeneration++;
            _recentRefreshTimer.Stop();
            _recentWatcher?.Dispose();
        };
        _ = RefreshRecentFilesAsync();
    }

    private void RecentStore_Changed(object sender, FileSystemEventArgs e) => DispatcherQueue.TryEnqueue(() =>
    {
        if (_recentsClosed || HomeView.Visibility != Visibility.Visible) return;
        _recentRefreshTimer.Stop();
        _recentRefreshTimer.Start();
    });

    private async Task RefreshRecentFilesAsync()
    {
        if (_recentsClosed) return;
        var generation = ++_recentGeneration;
        var entries = await WindowsRecentDocuments.ReadAsync();
        if (generation != _recentGeneration) return;
        RecentFilesList.Items.Clear();
        _recentGroups.Clear();
        var previews = new List<(WindowsRecentPdf Entry, Image Image, TextBlock Meta, string Opened)>();
        var today = DateTime.Today;
        foreach (var bucket in new[] { "Today", "Yesterday", "Earlier" })
        {
            var matching = entries.Where(entry => RecentBucket(entry.OpenedUtc.ToLocalTime().Date, today) == bucket).ToArray();
            if (matching.Length == 0) continue;
            var group = new StackPanel { Spacing = 8 };
            group.Children.Add(new Border { Child = new TextBlock { Text = bucket, Style = HomeStyle("DayChipTextStyle") }, Style = HomeStyle("DayChipStyle") });
            var flow = new Viewer.ToolbarPanel { ColumnSpacing = 10, RowSpacing = 10 }; // .home flow: 10/10 as in recents.rs
            group.Children.Add(flow);
            var cards = new List<Button>();
            foreach (var entry in matching)
            {
                var name = Path.GetFileName(entry.Path);
                var local = entry.OpenedUtc.ToLocalTime();
                var opened = bucket == "Earlier" ? $"Opened {local:d}" : $"Opened {bucket.ToLowerInvariant()}, {local:t}";
                var image = new Image { Stretch = Stretch.Uniform };
                var meta = new TextBlock { Text = opened, Style = HomeStyle("HomeMetaStyle") };
                // The 108px preview frame also sets the card's text width, as in Linux.
                var content = new StackPanel { Spacing = 6, Width = 108 };
                content.Children.Add(new Border { Child = image, Style = HomeStyle("RecentThumbStyle") });
                content.Children.Add(new TextBlock { Text = name, Style = HomeStyle("RecentNameStyle") });
                content.Children.Add(meta);
                var button = new Button { Content = content, Tag = name, Style = HomeStyle("RecentCardButtonStyle"), VerticalAlignment = VerticalAlignment.Stretch };
                Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(button, $"Open {name}");
                ToolTipService.SetToolTip(button, entry.Path);
                button.Click += async (_, _) => await OpenRecentFileAsync(entry.Path);
                flow.Children.Add(button);
                cards.Add(button);
                previews.Add((entry, image, meta, opened));
            }
            RecentFilesList.Items.Add(group);
            _recentGroups.Add((group, cards));
        }
        ApplyHomeFilter();
        if (_railDestination == "Recent" && HomeView.Visibility == Visibility.Visible) FocusFirstRecentCard();
        // Sequential: do not queue eight thumbnail opens ahead of the user's document.
        foreach (var preview in previews)
        {
            if (generation != _recentGeneration || _isBusy) return;
            try
            {
                var bytes = await File.ReadAllBytesAsync(preview.Entry.Path);
                if (generation != _recentGeneration || _isBusy) return;
                using var facade = new PdfDocumentFacade(new GeneratedPdfCore(), new FileDiagnosticLogger(FileDiagnosticLogger.DefaultPath));
                var result = await facade.OpenAsync(new DocumentSource(Path.GetFileName(preview.Entry.Path), bytes));
                if (!result.IsSuccess || result.Value is not { PageCount: > 0 } session) continue;
                var dpi = (uint)Math.Clamp(Math.Ceiling(24 * _rasterizationScale), 24, 96);
                var render = await facade.RenderPageAsync(session.SessionId, 0, dpi, false);
                if (generation != _recentGeneration) return;
                if (render.Value is not { } page) continue;
                var bitmap = await MaterializeBitmapAsync(page);
                if (generation != _recentGeneration) return;
                preview.Image.Source = bitmap;
                var pages = session.PageCount == 1 ? "1 page" : $"{session.PageCount} pages";
                preview.Meta.Text = $"{preview.Opened} · {pages}";
            }
            catch { /* Passwords, missing files and failed thumbnails leave a blank frame. */ }
        }
    }

    private static string RecentBucket(DateTime opened, DateTime today) => opened == today ? "Today" : opened == today.AddDays(-1) ? "Yesterday" : "Earlier";

    private void FilterRecentFiles(string query)
    {
        var shown = 0;
        foreach (var group in _recentGroups)
        {
            var count = 0;
            foreach (var card in group.Cards)
            {
                var visible = ((string)card.Tag).Contains(query, StringComparison.OrdinalIgnoreCase);
                card.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
                if (visible) count++;
            }
            group.Panel.Visibility = count > 0 ? Visibility.Visible : Visibility.Collapsed;
            shown += count;
        }
        RecentEmptyMessage.Text = _recentGroups.Count == 0
            ? "No recent documents yet. Open a PDF and it will show up here."
            : "No recent document matches that search.";
        RecentEmptyMessage.Visibility = shown == 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    private void FocusFirstRecentCard()
    {
        var card = _recentGroups.SelectMany(group => group.Cards).FirstOrDefault(card => card.Visibility == Visibility.Visible);
        if (card is not null) card.Focus(FocusState.Programmatic);
        else RecentFilesList.Focus(FocusState.Programmatic);
    }

    private async Task OpenRecentFileAsync(string path)
    {
        if (_isBusy || _shellPickingFile || _dialogOpen) return;
        _shellPickingFile = true;
        SetNavigationEnabled(false);
        try { await OpenStorageFileAsync(await StorageFile.GetFileFromPathAsync(path)); }
        catch (Exception error) { AnnotationStatus.Text = _facade.OpenReadFailure(error).Error!.Message; }
        finally
        {
            _shellPickingFile = false;
            SetNavigationEnabled(!_isBusy);
        }
        if (HomeView.Visibility == Visibility.Visible) await RefreshRecentFilesAsync();
    }
}
