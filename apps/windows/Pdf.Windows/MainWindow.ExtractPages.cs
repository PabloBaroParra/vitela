using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>
/// Extract pages: pulling a typed range of the open document's pages out
/// into a new PDF, without replacing the open session.
/// </summary>
/// <remarks>
/// <para>
/// The closest relative of <see cref="ExportImagesButton_Click"/>'s chain:
/// both ask a question while a dialog is open, both take a destination only
/// once every choice has been checked by the core, and both leave the open
/// document exactly as it was. They differ in what comes out — several image
/// files there, one PDF here — and in the destination itself: a single file
/// picked with <see cref="FileSavePicker"/>, not a folder.
/// </para>
/// <para>
/// The range grammar, the two refusal gates (`/P` bit 5 and the full-rewrite
/// check) and the prune itself are all the core's, shared with the Linux
/// shell's `write::extract` chain through `pdf_document::prune` — this shell
/// only asks the questions and writes the bytes it gets back.
/// </para>
/// </remarks>
public sealed partial class MainWindow
{
    private bool _extractingPages;

    private async void ExtractPagesButton_Click(object sender, RoutedEventArgs e)
    {
        if (_extractingPages || _isBusy || _dialogOpen || _session is not { PageCount: > 0 } session) return;
        _extractingPages = true;
        try
        {
            SetBusy(true);
            if (!await PrepareDocumentLifecycleAsync()) return;
            // Check the shared extraction/rewrite gates before offering the range dialog.
            var preflight = await _facade.PlanExtractPagesAsync(session.SessionId, new ExtractPagesRequest("1"));
            if (!preflight.IsSuccess)
            {
                AnnotationStatus.Text = preflight.Error!.Message;
                return;
            }
            var plan = await AskExtractPagesAsync(session);
            if (plan is null)
            {
                AnnotationStatus.Text = "Extract cancelled.";
                return;
            }
            if (_session?.SessionId != session.SessionId) return;
            var picker = new FileSavePicker { SuggestedFileName = "document", DefaultFileExtension = ".pdf", CommitButtonText = "Save" };
            picker.FileTypeChoices.Add("PDF", [".pdf"]);
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            var file = await picker.PickSaveFileAsync();
            if (file is null)
            {
                AnnotationStatus.Text = "Extract cancelled. No file was written.";
                return;
            }
            if (_session?.SessionId != session.SessionId) return;
            await WriteExtractedPagesAsync(session.SessionId, plan, file);
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
        }
        finally
        {
            _extractingPages = false;
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }

    private async Task WriteExtractedPagesAsync(string sessionId, ExtractPagesPlan plan, StorageFile file)
    {
        SetBusy(true);
        StorageFile? temporary = null;
        try
        {
            AnnotationStatus.Text = $"Extracting {plan.Pages.Count} {(plan.Pages.Count == 1 ? "page" : "pages")}...";
            var result = await _facade.ExtractPagesAsync(sessionId, plan.Pages);
            if (!result.IsSuccess)
            {
                AnnotationStatus.Text = result.Error!.Message;
                return;
            }

            // Same replace-through-a-temporary shape as an ordinary save: a
            // reader who cancels midway must never be left with a half
            // written file at the name they picked.
            var folder = await StorageFolder.GetFolderFromPathAsync(Path.GetDirectoryName(file.Path)!);
            temporary = await folder.CreateFileAsync($".{file.Name}.{Guid.NewGuid():N}.tmp", CreationCollisionOption.GenerateUniqueName);
            await FileIO.WriteBytesAsync(temporary, result.Value!);
            await temporary.MoveAndReplaceAsync(file);
            temporary = null;

            AnnotationStatus.Text = ExtractPagesWording.Summary(plan.Pages.Count, file.Path, plan.SourceIsSigned);
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
        }
        finally
        {
            try
            {
                if (temporary is not null) await temporary.DeleteAsync();
            }
            catch (Exception error)
            {
                AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
            }
            finally
            {
                SetBusy(false);
                RestoreAnnotationControls();
            }
        }
    }

    /// <summary>
    /// Asks which pages to extract. Stays open on a refused range — the
    /// core's own sentence, or one of the two permission refusals the plan
    /// checks before the grammar ever runs — so the reader corrects it in
    /// place rather than discovering it after picking a destination.
    /// </summary>
    private async Task<ExtractPagesPlan?> AskExtractPagesAsync(DocumentSession session)
    {
        var range = new TextBox
        {
            Name = "ExtractPageRange",
            Header = "Pages to extract",
            PlaceholderText = "1-3,7",
            MinWidth = 240,
        };

        var refusal = new TextBlock { TextWrapping = TextWrapping.Wrap, Visibility = Visibility.Collapsed };
        refusal.Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["SystemFillColorCriticalBrush"];

        var panel = new StackPanel { Spacing = 12, MaxWidth = 420 };
        panel.Children.Add(range);
        panel.Children.Add(new TextBlock
        {
            Text = $"This document has {session.PageCount} {(session.PageCount == 1 ? "page" : "pages")}.",
            TextWrapping = TextWrapping.Wrap,
            Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["TextFillColorSecondaryBrush"],
        });
        panel.Children.Add(refusal);

        var dialog = new ContentDialog
        {
            Title = "Extract pages",
            Content = panel,
            PrimaryButtonText = "Extract",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        dialog.Opened += (_, _) => range.Focus(FocusState.Programmatic);

        ExtractPagesPlan? plan = null;
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            try
            {
                var planned = await _facade.PlanExtractPagesAsync(session.SessionId, new ExtractPagesRequest(range.Text));
                if (planned.IsSuccess)
                {
                    plan = planned.Value;
                    return;
                }

                refusal.Text = planned.Error!.Message;
                refusal.Visibility = Visibility.Visible;
                args.Cancel = true;
            }
            finally
            {
                deferral.Complete();
            }
        };

        return await ShowModalAsync(dialog) == ContentDialogResult.Primary ? plan : null;
    }
}
