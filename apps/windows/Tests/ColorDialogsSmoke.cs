using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

public partial class App : Application
{
    private MainWindow? _window;
    public App() => BundledPdfium.PointCoreAtBundledLibrary();
    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var output = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")
            ?? throw new InvalidOperationException("Set VITELA_SMOKE_OUTPUT to an existing directory.");
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.ColorDialogsSmokeAsync();
            File.WriteAllText(Path.Combine(output, "color-dialogs-smoke.log"),
                "PASS annotation/field modal titles, RGB initialization/no alpha, Choose/Cancel, duplicate/busy guards, cancellation/no-op history, real RGB commits/Undo/Redo, stale annotation refusal and preserved field font/size.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "color-dialogs-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task ColorDialogsSmokeAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        static IEnumerable<DependencyObject> Descendants(DependencyObject parent)
        {
            yield return parent;
            for (var i = 0; i < VisualTreeHelper.GetChildrenCount(parent); i++)
                foreach (var child in Descendants(VisualTreeHelper.GetChild(parent, i))) yield return child;
        }
        async Task Wait(Func<bool> predicate, string message)
        {
            for (var i = 0; i < 200 && !predicate(); i++) await Task.Delay(25);
            Check(predicate(), message);
        }
        ContentDialog? CurrentDialog() => Content.XamlRoot is { } root
            ? VisualTreeHelper.GetOpenPopupsForXamlRoot(root).SelectMany(p => Descendants(p.Child)).OfType<ContentDialog>().FirstOrDefault()
            : null;
        async Task<ContentDialog> Dialog(string title, AnnotationColor initial)
        {
            await Wait(() => CurrentDialog()?.Title?.ToString() == title, "Missing " + title);
            var dialog = CurrentDialog()!;
            await Wait(() => Descendants(dialog).OfType<Button>().Any(b => b.Name == "PrimaryButton"), "Missing buttons");
            var picker = (ColorPicker)dialog.Content;
            Check(!picker.IsAlphaEnabled && picker.Color.A == 255 && picker.Color.R == initial.R
                && picker.Color.G == initial.G && picker.Color.B == initial.B, "Initial RGB/no-alpha");
            Check(dialog.PrimaryButtonText == "Choose" && dialog.CloseButtonText == "Cancel", "Choose/Cancel actions");
            Check(_dialogOpen && _isBusy && !OpenButton.IsEnabled && !PrintButton.IsEnabled, "Modal/busy ownership");
            return dialog;
        }
        static void Click(ContentDialog dialog, string name)
        {
            var button = Descendants(dialog).OfType<Button>().Single(b => b.Name == name);
            ((IInvokeProvider)FrameworkElementAutomationPeer.CreatePeerForElement(button).GetPattern(PatternInterface.Invoke)).Invoke();
        }
        async Task Finish(ContentDialog dialog, bool choose)
        {
            Click(dialog, choose ? "PrimaryButton" : "CloseButton");
            await Wait(() => !_dialogOpen && !_isBusy, "Modal did not release controls");
        }
        await Wait(() => Content.XamlRoot is not null, "Window not loaded");
        await OpenDocumentAsync(SampleDisplayName, await File.ReadAllBytesAsync(SamplePath));
        var sessionId = _session!.SessionId;
        await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.Shape, 0, new(40, 80, 100, 40), new(10, 20, 30)));
        var before = _annotationState!.Annotations.Last();
        _selectedAnnotationId = before.Id;
        UpdateAnnotationControls(_annotationState);
        AnnotationColorButton_Click(AnnotationColorButton, new RoutedEventArgs());
        var dialog = await Dialog("Choose annotation color", before.Color!);
        AnnotationColorButton_Click(AnnotationColorButton, new RoutedEventArgs());
        Check(CurrentDialog() == dialog, "Duplicate picker replaced the modal");
        ((ColorPicker)dialog.Content).Color = global::Windows.UI.Color.FromArgb(255, 0, 128, 255);
        await Finish(dialog, false);
        Check(_annotationState!.Annotations.Last().Color == before.Color && AnnotationColorButton.IsEnabled, "Cancel changed annotation");
        AnnotationColorButton_Click(AnnotationColorButton, new RoutedEventArgs());
        dialog = await Dialog("Choose annotation color", before.Color!);
        await Finish(dialog, true);
        await ApplyHistoryAndMetadataAsync(true);
        Check(_annotationState!.Annotations.Count == 0, "Cancel/no-op must not enter history");
        await ApplyHistoryAndMetadataAsync(false);
        _selectedAnnotationId = before.Id;
        UpdateAnnotationControls(_annotationState);
        AnnotationColorButton_Click(AnnotationColorButton, new RoutedEventArgs());
        dialog = await Dialog("Choose annotation color", before.Color!);
        ((ColorPicker)dialog.Content).Color = global::Windows.UI.Color.FromArgb(255, 0, 128, 255);
        await Finish(dialog, true);
        Check(_annotationState!.Annotations.Last().Color == new AnnotationColor(0, 128, 255), "RGB choice not committed");
        await ApplyHistoryAndMetadataAsync(true);
        Check(_annotationState!.Annotations.Last().Color == before.Color, "One-step color Undo");
        await ApplyHistoryAndMetadataAsync(false);
        _selectedAnnotationId = before.Id;
        UpdateAnnotationControls(_annotationState);
        AnnotationColorButton_Click(AnnotationColorButton, new RoutedEventArgs());
        dialog = await Dialog("Choose annotation color", new(0, 128, 255));
        Check((await _facade.EditAnnotationAsync(sessionId, new PdfCoreEdit.Move(before.Id, 1, 1))).IsSuccess, "Stale fixture move");
        ((ColorPicker)dialog.Content).Color = global::Windows.UI.Color.FromArgb(255, 40, 50, 60);
        await Finish(dialog, true);
        Check(AnnotationStatus.Text == "Color selection is no longer current.", "Stale target not refused");
        Check((await _facade.AnnotationStateAsync(sessionId)).Value!.Annotations.Last().Color == new AnnotationColor(0, 128, 255), "Stale color overwrote real core");

        SelectToolPage("Sign");
        await SetFormFieldPlacementAsync(FieldToPlace.Text, true);
        ClearFormFieldPlacement();
        Check((await _facade.AddTextFieldAsync(sessionId, 0, new(40, 400, 144, 36))).IsSuccess, "Field fixture");
        await RefreshFormFieldsAsync();
        var field = _formFieldState!.Fields.Last();
        SelectFormField(field.Id);
        var style = field.Style!;
        FormStyleColor_Click(FormStyleColor, new RoutedEventArgs());
        dialog = await Dialog("Choose field color", style.Color);
        ((ColorPicker)dialog.Content).Color = global::Windows.UI.Color.FromArgb(255, 80, 100, 120);
        await Finish(dialog, false);
        Check(SelectedFormField?.Style == style && FormStyleColor.IsEnabled, "Field Cancel");
        FormStyleColor_Click(FormStyleColor, new RoutedEventArgs());
        dialog = await Dialog("Choose field color", style.Color);
        await Finish(dialog, true);
        await ApplyHistoryAndMetadataAsync(true);
        Check(!_formFieldState!.Fields.Any(f => f.Id == field.Id), "Field no-op/Cancel must not add history");
        await ApplyHistoryAndMetadataAsync(false);
        SelectFormField(field.Id);
        FormStyleColor_Click(FormStyleColor, new RoutedEventArgs());
        dialog = await Dialog("Choose field color", style.Color);
        ((ColorPicker)dialog.Content).Color = global::Windows.UI.Color.FromArgb(255, 80, 100, 120);
        await Finish(dialog, true);
        Check(SelectedFormField?.Style == style with { Color = new(80, 100, 120) }, "Field color must preserve font/size");
        await ApplyHistoryAndMetadataAsync(true);
        Check(SelectedFormField?.Style == style, "Field color Undo");
        await ApplyHistoryAndMetadataAsync(false);
        Check(SelectedFormField?.Style?.Color == new AnnotationColor(80, 100, 120), "Field color Redo");
    }
}
