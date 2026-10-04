using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;
using Windows.Storage;
using Windows.Graphics.Imaging;

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
            await _window.OutputDialogsSmokeAsync(output);
            File.WriteAllText(Path.Combine(output, "output-dialogs-smoke.log"),
                "PASS Split labels/hint/focus, same-modal last-page cut correction, one counted overwrite Cancel/default/Replace, locked destination stop/count, two complete replaced parts reopen and pending history preserved; Extract labels/page-count/focus, same-modal empty/out-of-range correction, locked-destination preservation, atomic write/reopen and summary; Export All/Current/Pages default, range enabled/focus, same-modal invalid-range correction, PNG/JPEG and DPI defaults; real PNG/JPEG writes and decode, unique-name collision preservation, failed destination stops with completed count, live session/history preserved.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "output-dialogs-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task OutputDialogsSmokeAsync(string output)
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
        async Task<ContentDialog> Dialog(string title)
        {
            await Wait(() => CurrentDialog()?.Title?.ToString() == title, "Missing dialog: " + title);
            var dialog = CurrentDialog()!;
            await Wait(() => Descendants(dialog).OfType<Button>().Any(b => b.Name == "PrimaryButton"), "Dialog buttons missing");
            return dialog;
        }
        static void Click(ContentDialog dialog, string name)
        {
            var button = Descendants(dialog).OfType<Button>().Single(b => b.Name == name);
            var peer = FrameworkElementAutomationPeer.CreatePeerForElement(button);
            ((IInvokeProvider)peer.GetPattern(PatternInterface.Invoke)).Invoke();
        }
        await Wait(() => Content.XamlRoot is not null, "Window not loaded");
        await OpenDocumentAsync(SampleDisplayName, await File.ReadAllBytesAsync(SamplePath));
        var session = _session!;
        var asking = AskImageExportAsync(session);
        var dialog = await Dialog("Export pages as images");
        var panel = (StackPanel)dialog.Content;
        var pages = (RadioButtons)panel.Children[0];
        var range = Descendants(pages).OfType<TextBox>().Single(b => b.Name == "ExportPageRange");
        var settings = (StackPanel)panel.Children[1];
        var format = (ComboBox)settings.Children[0];
        var dpi = (NumberBox)settings.Children[1];
        Check(pages.SelectedIndex == 0 && !range.IsEnabled, "All default must disable range");
        Check(format.SelectedIndex == 0 && dpi.Value == 150 && dpi.Minimum == 72 && dpi.Maximum == 400, "Format/DPI defaults and bounds");
        pages.SelectedIndex = 2;
        await Wait(() => range.IsEnabled && range.FocusState != FocusState.Unfocused, "Custom selection must focus range");
        range.Text = "9999";
        Click(dialog, "PrimaryButton");
        var refusal = (TextBlock)panel.Children[2];
        await Wait(() => refusal.Visibility == Visibility.Visible && refusal.Text.Length > 0, "Invalid range must explain refusal");
        Check(!asking.IsCompleted && CurrentDialog() == dialog, "Invalid range must retain same modal");
        pages.SelectedIndex = 1;
        Check(!range.IsEnabled, "Current must disable custom entry");
        pages.SelectedIndex = 2;
        range.Text = "1";
        format.SelectedIndex = 1;
        dpi.Value = 72;
        Click(dialog, "PrimaryButton");
        var plan = await asking;
        Check(plan is { Dpi: 72, Format: ImageExportFormat.Jpeg } && plan.Files.Count == 1 && plan.Files[0].PageIndex == 0,
            "Corrected custom range must carry chosen format/DPI");
        var extracting = AskExtractPagesAsync(session);
        var extractDialog = await Dialog("Extract pages");
        var extractPanel = (StackPanel)extractDialog.Content;
        var extractRange = (TextBox)extractPanel.Children[0];
        Check(extractRange.Header?.ToString() == "Pages to extract" && extractRange.PlaceholderText == "1-3,7",
            "Extract labels/placeholder");
        Check(((TextBlock)extractPanel.Children[1]).Text == $"This document has {session.PageCount} {(session.PageCount == 1 ? "page" : "pages")}.",
            "Extract hint must show exact current page count");
        await Wait(() => extractRange.FocusState != FocusState.Unfocused, "Extract range must start focused");
        Click(extractDialog, "PrimaryButton");
        var extractRefusal = (TextBlock)extractPanel.Children[2];
        await Wait(() => extractRefusal.Visibility == Visibility.Visible, "Empty extraction must be refused inline");
        Check(extractRefusal.Text == "Type which pages to extract, for example 1-3,7." && !extracting.IsCompleted,
            "Empty extraction message/same modal");
        extractRange.Text = "9999";
        Click(extractDialog, "PrimaryButton");
        await Wait(() => extractRefusal.Text.Contains("9999"), "Out-of-range core message must reach same modal");
        extractRange.Text = "1";
        Click(extractDialog, "PrimaryButton");
        var extractPlan = await extracting;
        Check(extractPlan is not null && extractPlan.Pages.SequenceEqual(new uint[] { 0 }), "Corrected extraction range");
        var directory = Path.Combine(output, "output-smoke-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            var folder = await StorageFolder.GetFolderFromPathAsync(directory);
            var extractedFile = await folder.CreateFileAsync("extracted.pdf");
            await FileIO.WriteBytesAsync(extractedFile, [1, 2, 3]);
            using (var locked = new FileStream(extractedFile.Path, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
                await WriteExtractedPagesAsync(session.SessionId, extractPlan!, extractedFile);
            Check((await File.ReadAllBytesAsync(extractedFile.Path)).SequenceEqual(new byte[] { 1, 2, 3 }), "Failed extract write preserves destination");
            await WriteExtractedPagesAsync(session.SessionId, extractPlan!, extractedFile);
            Check(AnnotationStatus.Text == $"Extracted 1 page to {extractedFile.Path}.", "Extraction completion summary");
            using (var reopened = new GeneratedPdfCore().OpenFromBytes(await File.ReadAllBytesAsync(extractedFile.Path), null))
                Check(reopened.PageCount == 1, "Written extracted PDF must reopen with chosen count");
            foreach (var imageFormat in Enum.GetValues<ImageExportFormat>())
            {
                var planned = await _facade.PlanImageExportAsync(session.SessionId,
                    new ImageExportRequest(ImageExportPages.Current, "", 0, 72, imageFormat));
                Check(planned.IsSuccess, "Real export plan");
                var actualPlan = planned.Value!;
                var occupied = Path.Combine(directory, actualPlan.Files[0].FileName);
                await File.WriteAllBytesAsync(occupied, [1, 2, 3]);
                await WriteImagesAsync(session.SessionId, actualPlan, folder);
                Check(AnnotationStatus.Text.StartsWith("Exported 1 page as "), "Export completion: " + AnnotationStatus.Text);
                Check((await File.ReadAllBytesAsync(occupied)).SequenceEqual(new byte[] { 1, 2, 3 }), "Prior export must survive collision");
                var files = await folder.GetFilesAsync();
                var exported = files.Single(f => f.Name != Path.GetFileName(occupied) &&
                    f.FileType == (imageFormat == ImageExportFormat.Png ? ".png" : ".jpg"));
                using var stream = await exported.OpenReadAsync();
                var decoder = await BitmapDecoder.CreateAsync(stream);
                Check(decoder.PixelWidth > 0 && decoder.PixelHeight > 0, "Written image must decode");
            }
            var doomed = await folder.CreateFolderAsync("removed");
            await doomed.DeleteAsync();
            // StorageFolder may recreate a missing directory; a file at that path cannot be an output folder.
            await File.WriteAllBytesAsync(Path.Combine(directory, "removed"), [1]);
            await WriteImagesAsync(session.SessionId, plan!, doomed);
            Check(AnnotationStatus.Text.StartsWith("Export stopped after 0 written:"), "Destination failure must stop/count: " + AnnotationStatus.Text);
            Check(_session?.SessionId == session.SessionId && !(await _facade.HasUnsavedChangesAsync(session.SessionId)).Value,
                "Output must preserve live clean session");
            var blank = await _facade.CreateBlankAsync();
            Check(blank.IsSuccess, "Split blank fixture");
            var assembled = await _facade.EditPagesAsync(blank.Value!.SessionId, new PageEdit.InsertBlank(1, PageOrientation.Portrait));
            Check(assembled.IsSuccess && assembled.Value!.PageCount == 2, "Split two-page fixture");
            ShowOpenedDocument(assembled.Value!);
            var splitSession = _session!;
            var splitting = AskSplitPagesAsync(splitSession);
            var splitDialog = await Dialog("Split PDF");
            var splitPanel = (StackPanel)splitDialog.Content;
            var cuts = (TextBox)splitPanel.Children[0];
            var splitRefusal = (TextBlock)splitPanel.Children[2];
            Check(cuts.Header?.ToString() == "Split after page" && cuts.PlaceholderText == "3,7"
                && ((TextBlock)splitPanel.Children[1]).Text == "This document has 2 pages. Each cut starts a new file.", "Split words/hint");
            await Wait(() => cuts.FocusState != FocusState.Unfocused, "Split starts focused");
            cuts.Text = "2";
            Click(splitDialog, "PrimaryButton");
            await Wait(() => splitRefusal.Visibility == Visibility.Visible, "Last-page cut must be refused inline");
            Check(!splitting.IsCompleted && CurrentDialog() == splitDialog, "Split invalid cut retains same modal");
            cuts.Text = "1";
            Click(splitDialog, "PrimaryButton");
            var splitPlan = await splitting;
            Check(splitPlan is not null && splitPlan.Parts.Count == 2, "Valid cut produces two parts");
            foreach (var part in splitPlan!.Parts)
                await File.WriteAllBytesAsync(Path.Combine(directory, part.FileName), [1, 2, 3]);
            var replacing = AskSplitOverwritesAsync(splitPlan, folder);
            var replacementDialog = await Dialog("Replace 2 existing PDFs?");
            Check(replacementDialog.DefaultButton == ContentDialogButton.Close && replacementDialog.PrimaryButtonText == "Replace", "One counted replacement question with Cancel default");
            Click(replacementDialog, "CloseButton");
            Check(!await replacing && AnnotationStatus.Text == "Split cancelled.", "Split replacement cancellation");
            foreach (var part in splitPlan.Parts)
                Check((await File.ReadAllBytesAsync(Path.Combine(directory, part.FileName))).SequenceEqual(new byte[] { 1, 2, 3 }), "Cancel preserves every part");
            replacing = AskSplitOverwritesAsync(splitPlan, folder);
            Click(await Dialog("Replace 2 existing PDFs?"), "PrimaryButton");
            Check(await replacing, "Replacement must require explicit consent");
            using (var locked = new FileStream(Path.Combine(directory, splitPlan.Parts[0].FileName), FileMode.Open, FileAccess.ReadWrite, FileShare.None))
                await WriteSplitPagesAsync(splitSession.SessionId, splitPlan, folder);
            Check(AnnotationStatus.Text.StartsWith("Split stopped after 0 written:"), "Failed split reports completed count");
            foreach (var part in splitPlan.Parts)
                Check((await File.ReadAllBytesAsync(Path.Combine(directory, part.FileName))).SequenceEqual(new byte[] { 1, 2, 3 }), "Failed first part must preserve all original outputs");
            using (var locked = new FileStream(Path.Combine(directory, splitPlan.Parts[1].FileName), FileMode.Open, FileAccess.ReadWrite, FileShare.None))
                await WriteSplitPagesAsync(splitSession.SessionId, splitPlan, folder);
            Check(AnnotationStatus.Text.StartsWith("Split stopped after 1 written:") && AnnotationStatus.Text.Contains(splitPlan.Parts[1].FileName),
                "Failure after first part must name failed output and count earlier complete part");
            using (var partial = new GeneratedPdfCore().OpenFromBytes(await File.ReadAllBytesAsync(Path.Combine(directory, splitPlan.Parts[0].FileName)), null))
                Check(partial.PageCount == 1, "Earlier complete part survives later failure");
            Check((await File.ReadAllBytesAsync(Path.Combine(directory, splitPlan.Parts[1].FileName))).SequenceEqual(new byte[] { 1, 2, 3 }),
                "Failed later replacement must preserve its original bytes");
            await WriteSplitPagesAsync(splitSession.SessionId, splitPlan, folder);
            Check(AnnotationStatus.Text == $"Split into 2 PDFs in {folder.Path}.", "Split completion summary");
            foreach (var part in splitPlan.Parts)
            {
                using var reopened = new GeneratedPdfCore().OpenFromBytes(await File.ReadAllBytesAsync(Path.Combine(directory, part.FileName)), null);
                Check(reopened.PageCount == 1, "Each complete split part reopens with one page");
            }
            Check(_session?.SessionId == splitSession.SessionId && (await _facade.HasUnsavedChangesAsync(splitSession.SessionId)).Value,
                "Split output must preserve pending live history");
        }
        finally { Directory.Delete(directory, recursive: true); }
    }
}
