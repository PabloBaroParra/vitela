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
    private bool _compressingDocument;

    private async void CompressButton_Click(object sender, RoutedEventArgs e)
    {
        if (_compressingDocument || _isBusy || _dialogOpen || _session is not { } session) return;
        _compressingDocument = true;
        var sessionId = session.SessionId;
        StorageFile? temporary = null;
        try
        {
            SetBusy(true);
            if (!await PrepareDocumentLifecycleAsync()) return;
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
            var preset = await AskCompressionPresetAsync(session.SourceByteCount);
            if (preset is null)
            {
                AnnotationStatus.Text = "Compression cancelled.";
                return;
            }
            if (_session?.SessionId != sessionId) return;
            // Rewriting always asks the compression-specific core question, even without edits.
            var acknowledged = await AskSignatureLossAsync(compressing: true);
            if (acknowledged is null || _session?.SessionId != sessionId) return;
            SetBusy(true);
            AnnotationStatus.Text = "Compressing PDF...";
            var compressed = await _facade.CompressAsync(sessionId, preset.Value, acknowledged.Value);
            if (_session?.SessionId != sessionId) return;
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

            var picker = new FileSavePicker { SuggestedFileName = "document", DefaultFileExtension = ".pdf", CommitButtonText = "Save" };
            picker.FileTypeChoices.Add("PDF", [".pdf"]);
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            var file = await picker.PickSaveFileAsync();
            if (file is null)
            {
                AnnotationStatus.Text = "Compression cancelled. No file was written.";
                return;
            }
            if (_session?.SessionId != sessionId) return;

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
            _compressingDocument = false;
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
    private async Task<CompressionPreset?> AskCompressionPresetAsync(ulong? sourceByteCount)
    {
        var choices = new RadioButtons();
        foreach (var preset in Enum.GetValues<CompressionPreset>())
        {
            var (name, description) = CompressionWording.Describe(preset);
            var label = new StackPanel { Spacing = 2 };
            label.Children.Add(new TextBlock { Text = name, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold });
            label.Children.Add(new TextBlock { Text = description, TextWrapping = TextWrapping.Wrap, MaxWidth = 350, Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["TextFillColorSecondaryBrush"] });
            var choice = new RadioButton { Content = label, Tag = preset };
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(choice, name);
            choices.Items.Add(choice);
        }
        choices.SelectedIndex = Array.IndexOf(Enum.GetValues<CompressionPreset>(), CompressionPreset.Balanced);

        var panel = new StackPanel { Spacing = 12, MaxWidth = 420 };
        panel.Children.Add(new TextBlock
        {
            Text = sourceByteCount is { } bytes ? $"This file is {CompressionWording.HumanSize(bytes)} on disk." : "No source file size is available for this new document.",
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
        return (choices.SelectedItem as RadioButton)?.Tag as CompressionPreset? ?? CompressionPreset.Balanced;
    }
}
