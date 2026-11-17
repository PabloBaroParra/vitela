using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Pdf.Windows.Localization;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private bool _splittingPages;

    private async void SplitPagesButton_Click(object sender, RoutedEventArgs e)
    {
        if (_splittingPages || _isBusy || _dialogOpen || _session is not { PageCount: > 1 } session) return;
        _splittingPages = true;
        try
        {
            SetBusy(true);
            if (!await PrepareDocumentLifecycleAsync()) return;
            var preflight = await _facade.PlanSplitPagesAsync(session.SessionId, "1");
            if (!preflight.IsSuccess)
            {
                AnnotationStatus.Text = preflight.Error!.Message;
                return;
            }
            var plan = await AskSplitPagesAsync(session);
            if (plan is null)
            {
                AnnotationStatus.Text = I18n.T("split-cancelled");
                return;
            }
            if (_session?.SessionId != session.SessionId) return;
            var picker = new FolderPicker { CommitButtonText = I18n.T("split-accept") };
            picker.FileTypeFilter.Add("*");
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            var folder = await picker.PickSingleFolderAsync();
            if (folder is null)
            {
                AnnotationStatus.Text = I18n.T("split-nothing-written");
                return;
            }
            if (_session?.SessionId != session.SessionId || !await AskSplitOverwritesAsync(plan, folder)) return;
            if (_session?.SessionId != session.SessionId) return;
            await WriteSplitPagesAsync(session.SessionId, plan, folder);
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
        }
        finally
        {
            _splittingPages = false;
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }

    private async Task<bool> AskSplitOverwritesAsync(SplitPagesPlan plan, StorageFolder folder)
    {
        var existing = 0;
        foreach (var part in plan.Parts)
            if (await folder.TryGetItemAsync(part.FileName) is not null) existing++;
        if (existing == 0) return true;
        var dialog = new ContentDialog
        {
            Title = I18n.T("split-replace-title", ("count", existing)),
            Content = new TextBlock
            {
                Text = I18n.T("split-replace-detail", ("total", plan.Parts.Count), ("count", existing)),
                TextWrapping = TextWrapping.Wrap,
            },
            PrimaryButtonText = I18n.T("split-replace"),
            CloseButtonText = I18n.T("split-cancel"),
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = Content.XamlRoot,
        };
        if (await ShowModalAsync(dialog) == ContentDialogResult.Primary) return true;
        AnnotationStatus.Text = I18n.T("split-cancelled");
        return false;
    }

    private async Task WriteSplitPagesAsync(string sessionId, SplitPagesPlan plan, StorageFolder folder)
    {
        SetBusy(true);
        var written = 0;
        string? writingPart = null;
        StorageFile? temporary = null;
        try
        {
            foreach (var part in plan.Parts)
            {
                writingPart = part.FileName;
                AnnotationStatus.Text = I18n.T("split-writing-part", ("part", written + 1), ("total", plan.Parts.Count));
                var pages = Enumerable.Range((int)part.First, checked((int)(part.Last - part.First + 1)))
                    .Select(index => (uint)index).ToArray();
                var result = await _facade.ExtractPagesAsync(sessionId, pages);
                if (!result.IsSuccess)
                {
                    AnnotationStatus.Text = I18n.T("split-stopped", ("written", written), ("name", part.FileName), ("error", result.Error!.Message));
                    return;
                }

                temporary = await folder.CreateFileAsync($".vitela-{Guid.NewGuid():N}.tmp", CreationCollisionOption.FailIfExists);
                await FileIO.WriteBytesAsync(temporary, result.Value!);
                // The whole set's overwrite decision was made before the first part.
                // Only a complete temporary is moved to its final name.
                await temporary.MoveAsync(folder, part.FileName, NameCollisionOption.ReplaceExisting);
                temporary = null;
                written++;
            }

            AnnotationStatus.Text = I18n.T("split-summary", ("count", written), ("folder", folder.Path)) +
                (plan.SourceIsSigned ? " " + I18n.T("split-signed-note") : "");
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = I18n.T("split-stopped", ("written", written), ("name", writingPart ?? ""), ("error", _facade.SaveWriteFailure(error).Error!.Message));
        }
        finally
        {
            if (temporary is not null)
            {
                try { await temporary.DeleteAsync(); }
                catch { }
            }
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }

    private async Task<SplitPagesPlan?> AskSplitPagesAsync(DocumentSession session)
    {
        var cuts = new TextBox { Name = "SplitCutPoints", Header = I18n.T("split-cuts-label"), PlaceholderText = "3,7", MinWidth = 240 };
        var hint = new TextBlock { Text = I18n.T("split-range-hint", ("total", session.PageCount)), TextWrapping = TextWrapping.Wrap,
            Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["TextFillColorSecondaryBrush"] };
        var refusal = new TextBlock { TextWrapping = TextWrapping.Wrap, Visibility = Visibility.Collapsed };
        refusal.Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["SystemFillColorCriticalBrush"];
        var panel = new StackPanel { Spacing = 12, MaxWidth = 420 };
        panel.Children.Add(cuts);
        panel.Children.Add(hint);
        panel.Children.Add(refusal);

        var dialog = new ContentDialog
        {
            Title = I18n.T("split-dialog-title"),
            Content = panel,
            PrimaryButtonText = I18n.T("split-accept"),
            CloseButtonText = I18n.T("split-cancel"),
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        dialog.Opened += (_, _) => cuts.Focus(FocusState.Programmatic);
        SplitPagesPlan? plan = null;
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            try
            {
                var result = await _facade.PlanSplitPagesAsync(session.SessionId, cuts.Text);
                if (result.IsSuccess)
                {
                    plan = result.Value;
                    return;
                }
                refusal.Text = result.Error!.Message;
                refusal.Visibility = Visibility.Visible;
                args.Cancel = true;
            }
            finally { deferral.Complete(); }
        };
        return await ShowModalAsync(dialog) == ContentDialogResult.Primary ? plan : null;
    }
}
