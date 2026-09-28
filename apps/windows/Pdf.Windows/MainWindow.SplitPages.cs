using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private async void SplitPagesButton_Click(object sender, RoutedEventArgs e)
    {
        if (_isBusy || _session is not { PageCount: > 1 } session) return;
        var plan = await AskSplitPagesAsync(session);
        if (plan is null)
        {
            AnnotationStatus.Text = "Split cancelled.";
            return;
        }

        var picker = new FolderPicker();
        picker.FileTypeFilter.Add("*");
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        try
        {
            var folder = await picker.PickSingleFolderAsync();
            if (folder is null)
            {
                AnnotationStatus.Text = "Split cancelled. No file was written.";
                return;
            }
            await WriteSplitPagesAsync(session.SessionId, plan, folder);
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
        }
    }

    private async Task WriteSplitPagesAsync(string sessionId, SplitPagesPlan plan, StorageFolder folder)
    {
        SetBusy(true);
        var written = 0;
        StorageFile? temporary = null;
        try
        {
            foreach (var part in plan.Parts)
            {
                AnnotationStatus.Text = $"Writing part {written + 1} of {plan.Parts.Count}...";
                var pages = Enumerable.Range((int)part.First, checked((int)(part.Last - part.First + 1)))
                    .Select(index => (uint)index).ToArray();
                var result = await _facade.ExtractPagesAsync(sessionId, pages);
                if (!result.IsSuccess)
                {
                    AnnotationStatus.Text = $"Split stopped after {written} written: {result.Error!.Message}";
                    return;
                }

                temporary = await folder.CreateFileAsync($".vitela-{Guid.NewGuid():N}.tmp", CreationCollisionOption.FailIfExists);
                await FileIO.WriteBytesAsync(temporary, result.Value!);
                // A previous split is never overwritten; only a complete part
                // is moved to its final name, with a suffix on collision.
                await temporary.MoveAsync(folder, part.FileName, NameCollisionOption.GenerateUniqueName);
                temporary = null;
                written++;
            }

            AnnotationStatus.Text = $"Split into {written} PDFs in {folder.Path}." +
                (plan.SourceIsSigned ? " The original document is signed, so the signature in each part no longer verifies." : "");
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = $"Split stopped after {written} written: {_facade.SaveWriteFailure(error).Error!.Message}";
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
        var cuts = new TextBox { Header = "Split after page", PlaceholderText = "e.g. 3,7", MinWidth = 240 };
        var hint = new TextBlock { Text = $"This document has {session.PageCount} pages. Each cut starts a new file.", TextWrapping = TextWrapping.Wrap };
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
