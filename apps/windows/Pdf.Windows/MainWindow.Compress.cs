using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>
/// Compress: writing a smaller copy of the open document.
/// </summary>
/// <remarks>
/// <para>
/// Unlike Save and Protect, nothing is reopened afterwards. A compression
/// means "there is also a smaller copy over there"; the open session, its
/// pending edits and its undo history are exactly as they were.
/// </para>
/// <para>
/// The destination is asked for <em>last</em>. The core guarantees the
/// result is never larger, not that it is ever smaller, so whether there is a
/// file worth writing is only known once the compression has run. When
/// nothing smaller came out, no picker opens at all.
/// </para>
/// </remarks>
public sealed partial class MainWindow
{
    private async void CompressButton_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null) return;
        var sessionId = _session.SessionId;
        var displayName = _session.DisplayName;

        var refusal = await _facade.CompressionRefusalAsync(sessionId);
        if (!refusal.IsSuccess)
        {
            AnnotationStatus.Text = refusal.Error!.Message;
            return;
        }
        if (refusal.Value is { } reason)
        {
            AnnotationStatus.Text = reason;
            return;
        }

        var preset = await AskCompressionPresetAsync();
        if (preset is null)
        {
            AnnotationStatus.Text = "Compression cancelled.";
            return;
        }

        // The compression's own signature question, not Save's: a compressed
        // save rewrites a signed file even when nothing was edited.
        var signatureQuery = await _facade.CompressionWillInvalidateSignaturesAsync(sessionId);
        if (!signatureQuery.IsSuccess)
        {
            AnnotationStatus.Text = signatureQuery.Error!.Message;
            return;
        }

        var acknowledged = false;
        if (signatureQuery.Value)
        {
            var warning = new ContentDialog
            {
                Title = "Compressing will break this document's signature",
                Content = new TextBlock
                {
                    Text = "Compressing rewrites the file, so the compressed copy's digital signature will no longer verify. The open document is not changed.",
                    TextWrapping = TextWrapping.Wrap,
                },
                PrimaryButtonText = "Compress anyway",
                CloseButtonText = "Cancel",
                DefaultButton = ContentDialogButton.Close,
                XamlRoot = Content.XamlRoot,
            };
            if (await ShowModalAsync(warning) != ContentDialogResult.Primary) return;
            acknowledged = true;
        }

        SetBusy(true);
        StorageFile? temporary = null;
        try
        {
            AnnotationStatus.Text = "Compressing PDF...";
            var compressed = await _facade.CompressAsync(sessionId, preset.Value, acknowledged);
            if (!compressed.IsSuccess)
            {
                AnnotationStatus.Text = compressed.Error!.Message;
                return;
            }

            var result = compressed.Value!;
            if (!result.Reduced)
            {
                AnnotationStatus.Text = CompressionWording.NoGainSummary(result);
                return;
            }
            AnnotationStatus.Text = CompressionWording.ReductionSummary(result);

            var picker = new FileSavePicker();
            picker.FileTypeChoices.Add("PDF", [".pdf"]);
            picker.SuggestedFileName = $"{Path.GetFileNameWithoutExtension(displayName)}-compressed";
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            var file = await picker.PickSaveFileAsync();
            if (file is null)
            {
                AnnotationStatus.Text = "Compression cancelled. No file was written.";
                return;
            }

            AnnotationStatus.Text = "Writing the compressed PDF...";
            var folder = await StorageFolder.GetFolderFromPathAsync(Path.GetDirectoryName(file.Path)!);
            temporary = await folder.CreateFileAsync($".{file.Name}.{Guid.NewGuid():N}.tmp", CreationCollisionOption.GenerateUniqueName);
            await FileIO.WriteBytesAsync(temporary, result.Bytes);
            await temporary.MoveAndReplaceAsync(file);
            temporary = null;
            AnnotationStatus.Text = CompressionWording.WrittenSummary(result, file.Path);
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
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

    /// <summary>
    /// One radio per preset, Balanced pre-selected — the core's own "default
    /// offer". No predicted saving: nothing can answer that without running.
    /// </summary>
    private async Task<CompressionPreset?> AskCompressionPresetAsync()
    {
        var choices = new RadioButtons();
        foreach (var preset in Enum.GetValues<CompressionPreset>())
        {
            var (name, description) = CompressionWording.Describe(preset);
            var label = new StackPanel { Spacing = 2 };
            label.Children.Add(new TextBlock { Text = name, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold });
            label.Children.Add(new TextBlock { Text = description, TextWrapping = TextWrapping.Wrap, Opacity = 0.8 });
            choices.Items.Add(new RadioButton { Content = label, Tag = preset });
        }
        choices.SelectedIndex = Array.IndexOf(Enum.GetValues<CompressionPreset>(), CompressionPreset.Balanced);

        var panel = new StackPanel { Spacing = 12, MaxWidth = 420 };
        panel.Children.Add(new TextBlock
        {
            Text = "Writes a smaller copy. The open document stays as it is.",
            TextWrapping = TextWrapping.Wrap,
        });
        panel.Children.Add(choices);

        var dialog = new ContentDialog
        {
            Title = "Compress PDF",
            Content = panel,
            PrimaryButtonText = "Compress",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };

        if (await ShowModalAsync(dialog) != ContentDialogResult.Primary) return null;
        return (choices.SelectedItem as RadioButton)?.Tag as CompressionPreset?;
    }
}
