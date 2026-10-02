using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in harness exercises the real WinUI dialog and native annotation history.
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
            await _window.AnnotationSizeSmokeAsync(output);
            File.WriteAllText(Path.Combine(output, "annotation-size-smoke.log"), "PASS initial dimensions; invalid input; cancel/no-op preserve redo; precise resize; origin/selection; one-step undo/redo; saved PDF; stale session/annotation/selection; permission/busy/organize/modal/ink guards; restored controls.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "annotation-size-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task AnnotationSizeSmokeAsync(string output)
    {
        await WaitForSizeAsync(() => Content.XamlRoot is not null);
        await OpenDocumentAsync("Annotation size smoke", await File.ReadAllBytesAsync(SamplePath));
        if (_session is null) throw new Exception("Sample did not open.");
        await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.Shape, 0,
            new PdfCoreRect(40, 80, 100, 40), new PdfCoreColor(255, 220, 0)));
        var shape = _annotationState!.Annotations[^1];
        _selectedAnnotationId = shape.Id;
        UpdateAnnotationControls(_annotationState);
        if (!ResizeAnnotationButton.IsEnabled) throw new Exception("Resize unavailable for a shape.");
        await ApplyEditAsync(new PdfCoreEdit.Move(shape.Id, 12, 12));
        await ApplyHistoryAsync(true); // Preserve a redo branch across cancel and unchanged input.
        shape = _annotationState!.Annotations.Single(annotation => annotation.Id == shape.Id);

        var editing = ResizeSelectedAnnotationAsync();
        var dialog = await WaitForSizeDialogAsync();
        var fields = ((StackPanel)dialog.Content).Children.OfType<NumberBox>().ToArray();
        if (fields[0].Value != 100 || fields[1].Value != 40) throw new Exception("Initial dimensions changed.");
        foreach (var invalid in new[] { 0, -1, double.NaN })
        {
            fields[0].Value = invalid;
            if (dialog.IsPrimaryButtonEnabled) throw new Exception($"Invalid width {invalid} enabled Resize; actual value {fields[0].Value}.");
            fields[0].Value = 100;
            fields[1].Value = invalid;
            if (dialog.IsPrimaryButtonEnabled) throw new Exception($"Invalid height {invalid} enabled Resize; actual value {fields[1].Value}.");
            fields[1].Value = 40;
        }
        // NumberBox coerces infinity to its finite Maximum before ValueChanged.
        fields[0].Value = double.PositiveInfinity;
        if (!double.IsFinite(fields[0].Value)) throw new Exception("NumberBox retained an infinite width.");
        fields[0].Value = 100;
        dialog.Hide();
        await editing;
        await AssertSizeUnchangedAsync(shape);

        editing = ResizeSelectedAnnotationAsync();
        dialog = await WaitForSizeDialogAsync(dialog);
        InvokeResize(dialog);
        await editing;
        await AssertSizeUnchangedAsync(shape);

        editing = ResizeSelectedAnnotationAsync();
        dialog = await WaitForSizeDialogAsync(dialog);
        fields = ((StackPanel)dialog.Content).Children.OfType<NumberBox>().ToArray();
        fields[0].Value = 123.5;
        fields[1].Value = 67.25;
        InvokeResize(dialog);
        await editing;
        var resized = _annotationState!.Annotations.Single(annotation => annotation.Id == shape.Id);
        if (resized.Rect != new AnnotationRect(40, 80, 123.5, 67.25) || _selectedAnnotationId != shape.Id
            || _annotationState.CanRedo || _isBusy || !ResizeAnnotationButton.IsEnabled)
            throw new Exception("Resize lost origin, dimensions, selection or control/history state.");
        await ApplyHistoryAsync(true);
        if (_annotationState!.Annotations.Single(annotation => annotation.Id == shape.Id).Rect != shape.Rect)
            throw new Exception("One undo did not restore dimensions.");
        await ApplyHistoryAsync(false);
        if (_annotationState!.Annotations.Single(annotation => annotation.Id == shape.Id).Rect != resized.Rect)
            throw new Exception("Redo did not restore dimensions.");
        var saved = await _facade.SaveToDestinationAsync(_session.SessionId, bytes =>
            File.WriteAllBytesAsync(Path.Combine(output, "annotation-size-smoke.pdf"), bytes));
        if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);

        // Submission must still refer to the captured annotation and selection.
        foreach (var changeSelection in new[] { false, true })
        {
            editing = ResizeSelectedAnnotationAsync();
            dialog = await WaitForSizeDialogAsync(dialog);
            ((StackPanel)dialog.Content).Children.OfType<NumberBox>().First().Value = 200;
            if (changeSelection) _selectedAnnotationId = null;
            else await ApplyEditAsync(new PdfCoreEdit.Move(shape.Id, 1, 1));
            var before = _annotationState!;
            InvokeResize(dialog);
            await editing;
            if (!_annotationState!.Annotations.SequenceEqual(before.Annotations)) throw new Exception("Stale input overwrote annotation.");
            _selectedAnnotationId = shape.Id;
        }

        var settled = await _facade.SaveToDestinationAsync(_session.SessionId, _ => Task.CompletedTask);
        if (!settled.IsSuccess) throw new Exception(settled.Error!.Message);
        await RefreshAnnotationStateAsync();
        editing = ResizeSelectedAnnotationAsync();
        dialog = await WaitForSizeDialogAsync(dialog);
        ((StackPanel)dialog.Content).Children.OfType<NumberBox>().First().Value = 200;
        var oldSession = _session.SessionId;
        await OpenDocumentAsync("Replacement", await File.ReadAllBytesAsync(SamplePath));
        if (_session?.SessionId == oldSession) throw new Exception("Replacement did not open.");
        InvokeResize(dialog);
        await editing;
        if (_annotationState!.CanUndo || _annotationState.Annotations.Count != 0) throw new Exception("Old dialog edited new session.");

        await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.Ink, 0, new PdfCoreRect(0, 0, 0, 0),
            new PdfCoreColor(255, 220, 0), [new PdfCorePoint(10, 10), new PdfCorePoint(20, 20)]));
        _selectedAnnotationId = _annotationState!.Annotations[^1].Id;
        UpdateAnnotationControls(_annotationState);
        if (ResizeAnnotationButton.IsEnabled) throw new Exception("Ink enabled Resize.");
        await ResizeSelectedAnnotationAsync();
        await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.Shape, 0,
            new PdfCoreRect(40, 80, 100, 40), new PdfCoreColor(255, 220, 0)));
        _selectedAnnotationId = _annotationState!.Annotations[^1].Id;
        foreach (var guard in new[] { "permission", "busy", "organize", "modal" })
        {
            _annotationState = _annotationState with { EditingAllowed = guard != "permission" };
            _isBusy = guard == "busy";
            _organizing = guard == "organize";
            _dialogOpen = guard == "modal";
            UpdateAnnotationControls(_annotationState);
            if (guard != "modal" && ResizeAnnotationButton.IsEnabled) throw new Exception("Guard left Resize enabled.");
            await ResizeSelectedAnnotationAsync();
        }
        _isBusy = _organizing = _dialogOpen = false;
    }

    private async Task AssertSizeUnchangedAsync(Annotation expected)
    {
        var state = (await _facade.AnnotationStateAsync(_session!.SessionId)).Value!;
        if (state.Annotations.Single(annotation => annotation.Id == expected.Id).Rect != expected.Rect
            || !state.CanRedo || _dialogOpen) throw new Exception("Cancel/no-op changed dimensions, redo or modal state.");
    }

    private async Task<ContentDialog> WaitForSizeDialogAsync(ContentDialog? previous = null)
    {
        ContentDialog? dialog = null;
        await WaitForSizeAsync(() =>
        {
            dialog = VisualTreeHelper.GetOpenPopupsForXamlRoot(Content.XamlRoot)
                .SelectMany(popup => SizeDescendants(popup.Child)).OfType<ContentDialog>()
                .FirstOrDefault(item => item != previous && item.Title?.ToString()?.StartsWith("Resize annotation") == true);
            return dialog is not null;
        });
        return dialog!;
    }

    private static IEnumerable<DependencyObject> SizeDescendants(DependencyObject root)
    {
        yield return root;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
            foreach (var child in SizeDescendants(VisualTreeHelper.GetChild(root, i))) yield return child;
    }

    private static void InvokeResize(ContentDialog dialog)
    {
        var button = SizeDescendants(dialog).OfType<Button>().Single(item => item.Content?.ToString() == "Resize");
        ((IInvokeProvider)new ButtonAutomationPeer(button).GetPattern(PatternInterface.Invoke)).Invoke();
    }

    private static async Task WaitForSizeAsync(Func<bool> ready)
    {
        var deadline = DateTime.UtcNow.AddSeconds(10);
        while (!ready())
        {
            if (DateTime.UtcNow >= deadline) throw new TimeoutException("WinUI did not reach the expected state.");
            await Task.Delay(50);
        }
    }
}
