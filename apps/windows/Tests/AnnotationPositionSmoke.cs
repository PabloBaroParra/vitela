using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in harness exercises real WinUI, native movement and saved geometry.
public partial class App : Application
{
    private MainWindow? _window;
    public App() => BundledPdfium.PointCoreAtBundledLibrary();

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var output = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")
            ?? throw new InvalidOperationException("Set VITELA_SMOKE_OUTPUT to an existing output directory.");
        UnhandledException += (_, error) => File.AppendAllText(Path.Combine(output, "annotation-position-smoke.log"), "\nUNHANDLED " + error.Exception);
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.AnnotationPositionSmokeAsync(output);
            File.WriteAllText(Path.Combine(output, "annotation-position-smoke.log"), "PASS initial coordinates; invalid input; negative/zero coordinates; cancel/no-op preserve redo; exact movement; size/selection; undo/redo; rotated page; ink translation; saved PDF; stale session/annotation/selection; permission/busy/organize/modal guards; restored controls.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "annotation-position-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task AnnotationPositionSmokeAsync(string output)
    {
        await WaitForPositionAsync(() => Content.XamlRoot is not null);
        await OpenDocumentAsync("Annotation position smoke", await File.ReadAllBytesAsync(SamplePath));
        if (_session is null) throw new Exception("Sample did not open.");
        await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.Shape, 0,
            new PdfCoreRect(40, 80, 100, 40), new PdfCoreColor(255, 220, 0)));
        var shape = _annotationState!.Annotations[^1];
        _selectedAnnotationId = shape.Id;
        UpdateAnnotationControls(_annotationState);
        if (!MoveAnnotationButton.IsEnabled) throw new Exception("Move unavailable for a shape.");
        await ApplyEditAsync(new PdfCoreEdit.Move(shape.Id, 12, 12));
        await ApplyHistoryAsync(true);
        shape = _annotationState!.Annotations.Single(annotation => annotation.Id == shape.Id);

        var editing = MoveSelectedAnnotationAsync();
        var dialog = await WaitForPositionDialogAsync();
        var fields = ((StackPanel)dialog.Content).Children.OfType<NumberBox>().ToArray();
        if (fields[0].Value != 40 || fields[1].Value != 80) throw new Exception("Initial coordinates changed.");
        foreach (var field in fields)
        {
            var original = field.Value;
            field.Value = double.NaN;
            if (dialog.IsPrimaryButtonEnabled) throw new Exception("NaN enabled Move.");
            foreach (var coordinate in new[] { 0, -12.5 })
            {
                field.Value = coordinate;
                if (!dialog.IsPrimaryButtonEnabled) throw new Exception("Valid coordinate disabled Move.");
            }
            // NumberBox coerces infinity to its finite limit before ValueChanged.
            field.Value = double.PositiveInfinity;
            if (!double.IsFinite(field.Value)) throw new Exception("NumberBox retained infinity.");
            field.Value = original;
        }
        dialog.Hide();
        await editing;
        await AssertPositionUnchangedAsync(shape);

        editing = MoveSelectedAnnotationAsync();
        dialog = await WaitForPositionDialogAsync(dialog);
        InvokeMove(dialog);
        await editing;
        await AssertPositionUnchangedAsync(shape);

        editing = MoveSelectedAnnotationAsync();
        dialog = await WaitForPositionDialogAsync(dialog);
        fields = ((StackPanel)dialog.Content).Children.OfType<NumberBox>().ToArray();
        fields[0].Value = -12.5;
        fields[1].Value = 0;
        InvokeMove(dialog);
        await editing;
        var moved = _annotationState!.Annotations.Single(annotation => annotation.Id == shape.Id);
        if (moved.Rect != new AnnotationRect(-12.5, 0, 100, 40) || _selectedAnnotationId != shape.Id
            || _annotationState.CanRedo || _isBusy || !MoveAnnotationButton.IsEnabled)
            throw new Exception("Move lost coordinates, size, selection or control/history state.");
        await ApplyHistoryAsync(true);
        if (_annotationState!.Annotations.Single(annotation => annotation.Id == shape.Id).Rect != shape.Rect)
            throw new Exception("One undo did not restore position.");
        await ApplyHistoryAsync(false);
        if (_annotationState!.Annotations.Single(annotation => annotation.Id == shape.Id).Rect != moved.Rect)
            throw new Exception("Redo did not restore position.");

        // Coordinates remain in unrotated PDF space even when the page is rotated.
        var rotated = await _facade.EditPagesAsync(_session.SessionId, new PageEdit.Rotate(0, 90));
        if (!rotated.IsSuccess) throw new Exception(rotated.Error!.Message);
        _session = rotated.Value!;
        BuildPagePlaceholders(_session);
        await RefreshAnnotationStateAsync();
        editing = MoveSelectedAnnotationAsync();
        dialog = await WaitForPositionDialogAsync(dialog);
        fields = ((StackPanel)dialog.Content).Children.OfType<NumberBox>().ToArray();
        fields[0].Value = 23.5;
        fields[1].Value = 47.25;
        InvokeMove(dialog);
        await editing;
        if (_annotationState!.Annotations.Single(annotation => annotation.Id == shape.Id).Rect != new AnnotationRect(23.5, 47.25, 100, 40))
            throw new Exception("Rotated page changed PDF coordinates.");

        await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.Ink, 0, new PdfCoreRect(0, 0, 0, 0),
            new PdfCoreColor(255, 220, 0), [new PdfCorePoint(10, 20), new PdfCorePoint(30, 40)]));
        var ink = _annotationState!.Annotations[^1];
        _selectedAnnotationId = ink.Id;
        UpdateAnnotationControls(_annotationState);
        if (!MoveAnnotationButton.IsEnabled) throw new Exception("Ink disabled Move.");
        editing = MoveSelectedAnnotationAsync();
        dialog = await WaitForPositionDialogAsync(dialog);
        fields = ((StackPanel)dialog.Content).Children.OfType<NumberBox>().ToArray();
        if (fields[0].Value != 10 || fields[1].Value != 20) throw new Exception("Ink bounds changed.");
        fields[0].Value = -5;
        fields[1].Value = 0;
        InvokeMove(dialog);
        await editing;
        var movedInk = _annotationState!.Annotations.Single(annotation => annotation.Id == ink.Id);
        if (!movedInk.Points.SequenceEqual(new[] { new AnnotationPoint(-5, 0), new AnnotationPoint(15, 20) }))
            throw new Exception("Ink did not translate every point.");
        await ApplyHistoryAsync(true);
        if (!_annotationState!.Annotations.Single(annotation => annotation.Id == ink.Id).Points.SequenceEqual(ink.Points))
            throw new Exception("Ink undo did not restore points.");
        await ApplyHistoryAsync(false);
        var saved = await _facade.SaveToDestinationAsync(_session.SessionId, bytes =>
            File.WriteAllBytesAsync(Path.Combine(output, "annotation-position-smoke.pdf"), bytes));
        if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);

        _selectedAnnotationId = shape.Id;
        foreach (var guard in new[] { "annotation", "selection", "permission", "busy", "organize" })
        {
            editing = MoveSelectedAnnotationAsync();
            dialog = await WaitForPositionDialogAsync(dialog);
            ((StackPanel)dialog.Content).Children.OfType<NumberBox>().First().Value = 200;
            if (guard == "annotation") await ApplyEditAsync(new PdfCoreEdit.Move(shape.Id, 1, 1));
            if (guard == "selection") _selectedAnnotationId = ink.Id;
            if (guard == "permission") _annotationState = _annotationState! with { EditingAllowed = false };
            _isBusy = guard == "busy";
            _organizing = guard == "organize";
            var before = _annotationState!;
            InvokeMove(dialog);
            await editing;
            if (!_annotationState!.Annotations.SequenceEqual(before.Annotations)) throw new Exception("Guard allowed stale submission.");
            _isBusy = _organizing = false;
            _selectedAnnotationId = shape.Id;
            await RefreshAnnotationStateAsync();
        }

        var settled = await _facade.SaveToDestinationAsync(_session.SessionId, _ => Task.CompletedTask);
        if (!settled.IsSuccess) throw new Exception(settled.Error!.Message);
        await RefreshAnnotationStateAsync();
        editing = MoveSelectedAnnotationAsync();
        dialog = await WaitForPositionDialogAsync(dialog);
        ((StackPanel)dialog.Content).Children.OfType<NumberBox>().First().Value = 200;
        var oldSession = _session.SessionId;
        await OpenDocumentAsync("Replacement", await File.ReadAllBytesAsync(SamplePath));
        if (_session?.SessionId == oldSession) throw new Exception("Replacement did not open.");
        InvokeMove(dialog);
        await editing;
        if (_annotationState!.CanUndo || _annotationState.Annotations.Count != 0) throw new Exception("Old dialog edited new session.");

        await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.Shape, 0,
            new PdfCoreRect(40, 80, 100, 40), new PdfCoreColor(255, 220, 0)));
        _selectedAnnotationId = _annotationState!.Annotations[^1].Id;
        foreach (var guard in new[] { "permission", "busy", "organize", "modal", "selection" })
        {
            _annotationState = _annotationState with { EditingAllowed = guard != "permission" };
            _isBusy = guard == "busy";
            _organizing = guard == "organize";
            _dialogOpen = guard == "modal";
            if (guard == "selection") _selectedAnnotationId = null;
            UpdateAnnotationControls(_annotationState);
            if (guard != "modal" && MoveAnnotationButton.IsEnabled) throw new Exception("Guard left Move enabled.");
            await MoveSelectedAnnotationAsync();
        }
        _isBusy = _organizing = _dialogOpen = false;
    }

    private async Task AssertPositionUnchangedAsync(Annotation expected)
    {
        var state = (await _facade.AnnotationStateAsync(_session!.SessionId)).Value!;
        if (state.Annotations.Single(annotation => annotation.Id == expected.Id).Rect != expected.Rect
            || !state.CanRedo || _dialogOpen) throw new Exception("Cancel/no-op changed position, redo or modal state.");
    }

    private async Task<ContentDialog> WaitForPositionDialogAsync(ContentDialog? previous = null)
    {
        ContentDialog? dialog = null;
        await WaitForPositionAsync(() =>
        {
            dialog = VisualTreeHelper.GetOpenPopupsForXamlRoot(Content.XamlRoot)
                .SelectMany(popup => PositionDescendants(popup.Child)).OfType<ContentDialog>()
                .FirstOrDefault(item => item != previous && item.Title?.ToString()?.StartsWith("Move annotation") == true);
            return dialog is not null;
        });
        return dialog!;
    }

    private static IEnumerable<DependencyObject> PositionDescendants(DependencyObject root)
    {
        yield return root;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
            foreach (var child in PositionDescendants(VisualTreeHelper.GetChild(root, i))) yield return child;
    }

    private static void InvokeMove(ContentDialog dialog)
    {
        var button = PositionDescendants(dialog).OfType<Button>().Single(item => item.Content?.ToString() == "Move");
        ((IInvokeProvider)new ButtonAutomationPeer(button).GetPattern(PatternInterface.Invoke)).Invoke();
    }

    private static async Task WaitForPositionAsync(Func<bool> ready)
    {
        var deadline = DateTime.UtcNow.AddSeconds(10);
        while (!ready())
        {
            if (DateTime.UtcNow >= deadline) throw new TimeoutException("WinUI did not reach the expected state.");
            await Task.Delay(50);
        }
    }
}
