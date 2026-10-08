using System.Runtime.InteropServices.WindowsRuntime;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.Graphics.Imaging;
using Windows.Storage.Streams;

namespace Pdf.Windows;

// Opt-in WinUI runtime harness; compiled instead of App.xaml.cs via the targets file.
public partial class App : Application
{
    private MainWindow? _window;
    public App() => BundledPdfium.PointCoreAtBundledLibrary();

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var output = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")
            ?? throw new InvalidOperationException("Set VITELA_SMOKE_OUTPUT to an existing output directory.");
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.FreeTextSmokeAsync(output);
            File.WriteAllText(Path.Combine(output, "freetext-smoke.log"), "PASS arming; blank validation; cancel without history; encoding gap keeps the dialog open; click placement; core lines on the overlay; edit/no-op/undo/redo; resize rewrap and minimum; saved PDF; stale session; guards; rotated page.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "freetext-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task FreeTextSmokeAsync(string output)
    {
        await WaitForAsync(() => Content.XamlRoot is not null);
        await OpenDocumentAsync("FreeText smoke", await File.ReadAllBytesAsync(SamplePath));
        if (_session is null) throw new Exception("Sample did not open.");
        await RefreshAnnotationStateAsync();
        var before = _annotationState!;
        var click = new AnnotationPoint(60, 700);

        Arm(AnnotationKind.FreeText);
        if (_armedAnnotation != AnnotationKind.FreeText || FreeTextButton.IsChecked != true) throw new Exception("The Text box tool did not arm.");
        Arm(null);

        // Blank text cannot be added; Cancel records nothing.
        var placement = PlaceFreeTextAsync(0, click, click);
        var dialog = await WaitForFreeTextDialogAsync();
        var text = FreeTextBox(dialog);
        if (dialog.IsPrimaryButtonEnabled || !text.AcceptsReturn) throw new Exception("Empty text box enabled, or input is not multiline.");
        text.Text = " \r\n\t ";
        await Task.Delay(100);
        if (dialog.IsPrimaryButtonEnabled) throw new Exception("Whitespace enabled Add.");
        text.Text = "Discard me";
        dialog.Hide();
        await placement;
        var canceled = (await _facade.AnnotationStateAsync(_session.SessionId)).Value!;
        if (canceled.Annotations.Count != before.Annotations.Count || canceled.CanUndo != before.CanUndo) throw new Exception("Cancel changed state.");

        // A character the font cannot show keeps the dialog open, names it, and keeps the text.
        placement = PlaceFreeTextAsync(0, click, click);
        dialog = await WaitForFreeTextDialogAsync(dialog);
        text = FreeTextBox(dialog);
        text.Text = "Hola 日本";
        await WaitForAsync(() => dialog.IsPrimaryButtonEnabled);
        InvokePrimary(dialog, "Add");
        await WaitForAsync(() => FreeTextProblem(dialog).Visibility == Visibility.Visible);
        if (!FreeTextProblem(dialog).Text.Contains('日')) throw new Exception("The refusal did not name the character: " + FreeTextProblem(dialog).Text);
        if (text.Text != "Hola 日本") throw new Exception("The typed text was lost on refusal.");
        if (_annotationState!.Annotations.Count != before.Annotations.Count) throw new Exception("A refused add left an annotation.");

        const string Sentence = "Canción, años: ¿qué tal? Ü € ñ — esta línea es larga para forzar el ajuste en la caja";
        text.Text = Sentence;
        await WaitForAsync(() => FreeTextProblem(dialog).Visibility == Visibility.Collapsed && dialog.IsPrimaryButtonEnabled);
        InvokePrimary(dialog, "Add");
        await placement;
        if (_annotationState!.Annotations.Count != before.Annotations.Count + 1 || !_annotationState.CanUndo) throw new Exception("Add did not record one annotation.");
        var box = _annotationState.Annotations[^1];
        var (pageW, pageH) = FreeTextInput.UnrotatedPageSize(_session.Pages[0]);
        var expectedRect = FreeTextInput.PlacementRect(click, click, pageW, pageH);
        if (box.Kind != AnnotationKind.FreeText || box.Rect != expectedRect) throw new Exception($"Placement geometry wrong: {box.Rect} vs {expectedRect}.");
        if (box.Contents != Sentence) throw new Exception("Contents lost.");
        if (box.Layout is not { Lines.Count: >= 2 } layout) throw new Exception("The core wrapped nothing: " + box.Layout?.Lines.Count);
        if (_selectedAnnotationId != box.Id) throw new Exception("The new box is not selected.");
        var live = _facade.LayoutFreeText(Sentence, expectedRect.Width, expectedRect.Height).Value!;
        if (!live.Lines.Select(l => l.Text).SequenceEqual(layout.Lines.Select(l => l.Text))) throw new Exception("Stored lines differ from the live layout call.");

        // The overlay draws exactly the core's lines.
        RedrawAnnotations();
        CheckOverlayLines(0, layout);
        await Task.Delay(1500);
        await SnapshotPageAsync(Path.Combine(output, "freetext-smoke-upright.png"));

        // Edit: same text is no history; new text is one undo step.
        var undoDepthProbe = _annotationState.CanUndo;
        var editing = EditSelectedFreeTextAsync();
        dialog = await WaitForFreeTextDialogAsync(dialog, "Edit text box — page 1");
        text = FreeTextBox(dialog);
        if (text.Text != Sentence) throw new Exception("Edit dialog is not prefilled.");
        InvokePrimary(dialog, "Save");
        await editing;
        if (_annotationState!.Annotations[^1].Contents != Sentence) throw new Exception("Unchanged save altered the text.");
        editing = EditSelectedFreeTextAsync();
        dialog = await WaitForFreeTextDialogAsync(dialog, "Edit text box — page 1");
        text = FreeTextBox(dialog);
        text.Text = "Segunda versión\nde dos líneas";
        await WaitForAsync(() => dialog.IsPrimaryButtonEnabled);
        InvokePrimary(dialog, "Save");
        await editing;
        box = _annotationState.Annotations[^1];
        if (box.Contents != "Segunda versión\nde dos líneas" || box.Layout!.Lines.Count != 2) throw new Exception("Edit did not retype and relayout.");
        await ApplyHistoryAsync(true);
        if (_annotationState!.Annotations[^1].Contents != Sentence) throw new Exception("One undo did not restore the text (the no-op saved an entry, or the edit took two).");
        await ApplyHistoryAsync(false);
        if (_annotationState!.Annotations[^1].Contents != "Segunda versión\nde dos líneas") throw new Exception("Redo failed.");

        // Resize: narrower rewraps in the core; below the minimum is refused.
        await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.FreeText, 0, new PdfCoreRect(60, 500, 200, 50), new PdfCoreColor(0, 0, 0), Contents: Sentence));
        var wide = _annotationState!.Annotations[^1];
        var narrow = FreeTextInput.ResizedRect(new AnnotationPoint(60, 500), new AnnotationPoint(60 + 90, 550));
        await ApplyEditAsync(new PdfCoreEdit.Resize(wide.Id, new PdfCoreRect(narrow.X, narrow.Y, narrow.Width, narrow.Height)));
        var rewrapped = _annotationState!.Annotations.Single(a => a.Id == wide.Id);
        if (rewrapped.Layout!.Lines.Count <= wide.Layout!.Lines.Count) throw new Exception("Narrower box did not rewrap.");
        var tiny = FreeTextInput.ResizedRect(new AnnotationPoint(60, 500), new AnnotationPoint(61, 501));
        await ApplyEditAsync(new PdfCoreEdit.Resize(wide.Id, new PdfCoreRect(tiny.X, tiny.Y, tiny.Width, tiny.Height)));
        if (_annotationState!.Annotations.Single(a => a.Id == wide.Id).Rect is not { } floor || floor.Width < FreeTextInput.MinWidthPt || floor.Height < FreeTextInput.MinHeightPt)
            throw new Exception("Resize went below the minimum.");

        var saved = await _facade.SaveToDestinationAsync(_session.SessionId, bytes => File.WriteAllBytesAsync(Path.Combine(output, "freetext-smoke.pdf"), bytes));
        if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);
        var savedText = System.Text.Encoding.Latin1.GetString(await File.ReadAllBytesAsync(Path.Combine(output, "freetext-smoke.pdf")));
        if (!savedText.Contains("/FreeText") || !savedText.Contains("/AP")) throw new Exception("The saved file has no FreeText appearance.");

        // A rotated page turns the overlay with it.
        var rotated = await _facade.EditPagesAsync(_session.SessionId, new PageEdit.Rotate(0, 90));
        if (!rotated.IsSuccess) throw new Exception(rotated.Error!.Message);
        _session = rotated.Value!;
        BuildPagePlaceholders(_session);
        await RefreshAnnotationStateAsync();
        _selectedAnnotationId = null;
        RedrawAnnotations();
        CheckOverlayLines(0, _annotationState!.Annotations.Where(a => a.Kind == AnnotationKind.FreeText).First().Layout!);
        await Task.Delay(2000);
        await SnapshotPageAsync(Path.Combine(output, "freetext-smoke-rotated.png"));

        // Saving clears the dirty state, so opening a replacement does not ask about unsaved work.
        var rotatedSave = await _facade.SaveToDestinationAsync(_session.SessionId, bytes => File.WriteAllBytesAsync(Path.Combine(output, "freetext-smoke-rotated.pdf"), bytes));
        if (!rotatedSave.IsSuccess) throw new Exception(rotatedSave.Error!.Message);

        // Stale session: the dialog's result must not reach a replacement document.
        await OpenDocumentAsync("Placement guards", await File.ReadAllBytesAsync(SamplePath));
        placement = PlaceFreeTextAsync(0, click, click);
        dialog = await WaitForFreeTextDialogAsync(dialog);
        FreeTextBox(dialog).Text = "Wrong session";
        await WaitForAsync(() => dialog.IsPrimaryButtonEnabled);
        await OpenDocumentAsync("Replacement", await File.ReadAllBytesAsync(SamplePath));
        var replacementCount = _annotationState!.Annotations.Count;
        InvokePrimary(dialog, "Add");
        await placement;
        if (_annotationState!.Annotations.Count != replacementCount || _annotationState.CanUndo) throw new Exception("A stale dialog edited the replacement session.");

        // Guards open no dialog.
        _annotationState = _annotationState with { EditingAllowed = false };
        await PlaceFreeTextAsync(0, click, click);
        _annotationState = _annotationState with { EditingAllowed = true };
        _isBusy = true;
        await PlaceFreeTextAsync(0, click, click);
        _isBusy = false;
        _organizing = true;
        await PlaceFreeTextAsync(0, click, click);
        _organizing = false;
        if (_dialogOpen || _annotationState.CanUndo) throw new Exception("A guard opened a dialog or edited history.");
    }

    private void CheckOverlayLines(int pageIndex, FreeTextLayout layout)
    {
        var drawn = _slots[pageIndex].Annotations.Children.OfType<Canvas>()
            .Where(canvas => canvas.Children.OfType<TextBlock>().Any())
            .Select(canvas => canvas.Children.OfType<TextBlock>().Select(block => block.Text).ToArray())
            .ToArray();
        if (!drawn.Any(lines => lines.SequenceEqual(layout.Lines.Select(line => line.Text))))
            throw new Exception("The overlay does not draw the core's lines.");
    }

    private async Task SnapshotPageAsync(string path)
    {
        var target = new RenderTargetBitmap();
        await target.RenderAsync(_slots[0].Container);
        var pixels = await target.GetPixelsAsync();
        using var stream = new InMemoryRandomAccessStream();
        var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
        encoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied, (uint)target.PixelWidth, (uint)target.PixelHeight, 96, 96, pixels.ToArray());
        await encoder.FlushAsync();
        stream.Seek(0);
        var bytes = new byte[stream.Size];
        await stream.ReadAsync(bytes.AsBuffer(), (uint)bytes.Length, InputStreamOptions.None);
        await File.WriteAllBytesAsync(path, bytes);
    }

    private static TextBox FreeTextBox(ContentDialog dialog) => ((StackPanel)dialog.Content).Children.OfType<TextBox>().Single();
    private static TextBlock FreeTextProblem(ContentDialog dialog) => ((StackPanel)dialog.Content).Children.OfType<TextBlock>().Single();

    private async Task<ContentDialog> WaitForFreeTextDialogAsync(ContentDialog? previous = null, string? title = null)
    {
        ContentDialog? dialog = null;
        await WaitForAsync(() =>
        {
            dialog = VisualTreeHelper.GetOpenPopupsForXamlRoot(Content.XamlRoot)
                .Select(popup => FindFreeTextDialog(popup.Child)).FirstOrDefault(item => item is not null && item != previous && (title is null || item.Title?.ToString() == title));
            return dialog is not null;
        });
        return dialog!;
    }

    private static ContentDialog? FindFreeTextDialog(DependencyObject root)
    {
        if (root is ContentDialog dialog) return dialog;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
            if (FindFreeTextDialog(VisualTreeHelper.GetChild(root, i)) is { } found) return found;
        return null;
    }

    private static void InvokePrimary(ContentDialog dialog, string label)
    {
        var buttons = new List<Button>();
        void collect(DependencyObject root)
        {
            if (root is Button button) buttons.Add(button);
            for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++) collect(VisualTreeHelper.GetChild(root, i));
        }
        collect(dialog);
        var primary = buttons.Single(button => button.Content?.ToString() == label);
        ((IInvokeProvider)new ButtonAutomationPeer(primary).GetPattern(PatternInterface.Invoke)).Invoke();
    }

    private static async Task WaitForAsync(Func<bool> ready)
    {
        var deadline = DateTime.UtcNow.AddSeconds(10);
        while (!ready())
        {
            if (DateTime.UtcNow >= deadline) throw new TimeoutException("WinUI did not reach the expected state.");
            await Task.Delay(50);
        }
    }
}
