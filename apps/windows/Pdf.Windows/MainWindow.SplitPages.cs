using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
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
                AnnotationStatus.Text = "Split cancelled.";
                return;
            }
            if (_session?.SessionId != session.SessionId) return;
            var picker = new FolderPicker { CommitButtonText = "Split" };
            picker.FileTypeFilter.Add("*");
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            var folder = await picker.PickSingleFolderAsync();
            if (folder is null)
            {
                AnnotationStatus.Text = "Split cancelled. No file was written.";
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
            Title = $"Replace {existing} existing {(existing == 1 ? "PDF" : "PDFs")}?",
            Content = new TextBlock
            {
                Text = $"Splitting into this folder writes {plan.Parts.Count} files named after the document, and {existing} of them already exist there.",
                TextWrapping = TextWrapping.Wrap,
            },
            PrimaryButtonText = "Replace",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = Content.XamlRoot,
        };
        if (await ShowModalAsync(dialog) == ContentDialogResult.Primary) return true;
        AnnotationStatus.Text = "Split cancelled.";
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
                AnnotationStatus.Text = $"Writing part {written + 1} of {plan.Parts.Count}...";
                var pages = Enumerable.Range((int)part.First, checked((int)(part.Last - part.First + 1)))
                    .Select(index => (uint)index).ToArray();
                var result = await _facade.ExtractPagesAsync(sessionId, pages);
                if (!result.IsSuccess)
                {
                    AnnotationStatus.Text = $"Split stopped after {written} written: {part.FileName} could not be written: {result.Error!.Message}";
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

            AnnotationStatus.Text = $"Split into {written} PDFs in {folder.Path}." +
                (plan.SourceIsSigned ? " The original document is signed, so the signature in each part no longer verifies." : "");
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = $"Split stopped after {written} written: {writingPart} could not be written: {_facade.SaveWriteFailure(error).Error!.Message}";
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
        var cuts = new TextBox { Name = "SplitCutPoints", Header = "Split after page", PlaceholderText = "3,7", MinWidth = 240 };
        var hint = new TextBlock { Text = $"This document has {session.PageCount} pages. Each cut starts a new file.", TextWrapping = TextWrapping.Wrap,
            Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["TextFillColorSecondaryBrush"] };
        var refusal = new TextBlock { TextWrapping = TextWrapping.Wrap, Visibility = Visibility.Collapsed };
        refusal.Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["SystemFillColorCriticalBrush"];
        var panel = new StackPanel { Spacing = 12, MaxWidth = 420 };
        panel.Children.Add(cuts);
        panel.Children.Add(hint);
        panel.Children.Add(refusal);

        var dialog = new ContentDialog
        {
            Title = "Split PDF",
            Content = panel,
            PrimaryButtonText = "Split",
            CloseButtonText = "Cancel",
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
