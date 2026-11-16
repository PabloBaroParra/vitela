using System.Text;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in native runtime harness, the same replacement-entry-point pattern as
// the other smoke tests: bold, italic and underline toggles on existing text,
// driven through the real Edit card handlers and the real core.
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
            await _window.TextStyleSmokeAsync();
            File.WriteAllText(Path.Combine(output, "text-style-smoke.log"),
                "PASS a selected block turns bold, the Bold toggle shows it pressed and the block stays selected for a second "
                + "press; a selection splitting a run in the open editor is refused; a selection covering one run whole "
                + "restyles only that run and leaves its block selected; a press with the caret in the editor reopens it "
                + "over the restyled text in its new weight; text typed just before a toggle survives it; a native bold with no regular face is pressed and disabled; "
                + "the save reopens with every look.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "text-style-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task TextStyleSmokeAsync()
    {
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        if (Content.XamlRoot is null) throw new Exception("Window did not load.");
        await OpenTextStyleSmokeAsync(StyledPdf());
        SetContentEditMode(true);

        // A selected block: bold for both runs, the button pressed, still selected.
        SelectTextBlock(await StyleBlockAsync("Hello world"));
        await ToggleTextStyleAsync(TextStyleToggle.Bold);
        if (_selectedTextBlock?.Text != "Hello world") throw new Exception($"The restyled block is not selected. Status: {AnnotationStatus.Text}");
        await RequireLookAsync("Hello", bold: true, italic: false, underline: false);
        await RequireLookAsync(" world", bold: true, italic: false, underline: false);
        await SettleTogglesAsync();
        if (_boldToggle!.IsChecked != true || !_boldToggle.IsEnabled) throw new Exception("Bold does not show pressed for a bold block.");

        // A second press on the still-selected block stacks.
        await ToggleTextStyleAsync(TextStyleToggle.Underline);
        await RequireLookAsync("Hello", bold: true, italic: false, underline: true);

        // In the open editor: part of a run is refused, a whole run restyles alone.
        await RetypeSelectedTextBlockAsync();
        var editor = _pump.Box ?? throw new Exception("F2 opened no editor over the block.");
        editor.Box.Select(1, 2);
        await ToggleTextStyleAsync(TextStyleToggle.Italic);
        if (!AnnotationStatus.Text.Contains("styled as one piece")) throw new Exception($"A split run was not refused: {AnnotationStatus.Text}");
        editor.Box.Select(editor.Box.Text.IndexOf("world", StringComparison.Ordinal), "world".Length);
        await ToggleTextStyleAsync(TextStyleToggle.Italic);
        if (_pump.Box is not null) throw new Exception("A restyle of some of the editor's runs left it open.");
        if (_selectedTextBlock?.Text != "Hello world") throw new Exception("The editor's block was not left selected.");
        await RequireLookAsync(" world", bold: true, italic: true, underline: true);
        await RequireLookAsync("Hello", bold: true, italic: false, underline: true);

        // The caret alone: every run, and the editor opens again in the new face.
        await RetypeSelectedTextBlockAsync();
        (_pump.Box ?? throw new Exception("No editor.")).Box.Select(2, 0);
        await ToggleTextStyleAsync(TextStyleToggle.Bold);
        var reopened = _pump.Box ?? throw new Exception($"The editor did not reopen after a whole restyle. Status: {AnnotationStatus.Text}");
        if (reopened.Box.SelectionStart != 2) throw new Exception("The caret moved across the restyle.");
        await RequireLookAsync("Hello", bold: false, italic: false, underline: true);
        for (var attempt = 0; reopened.Box.FontWeight.Weight != 400 && attempt < 40; attempt++) await Task.Delay(50);
        if (reopened.Box.FontWeight.Weight != 400) throw new Exception("The reopened editor is not drawn in the new weight.");
        await CommitContentEditorAsync();

        // A native bold with no regular sibling: pressed, disabled, refused.
        SelectTextBlock(await StyleBlockAsync("Heavy"));
        await SettleTogglesAsync();
        if (_boldToggle.IsChecked != true || _boldToggle.IsEnabled) throw new Exception("A fixed bold is not shown pressed and disabled.");
        await ToggleTextStyleAsync(TextStyleToggle.Bold);
        if (!AnnotationStatus.Text.Contains("cannot be made regular")) throw new Exception($"A fixed bold was not refused: {AnnotationStatus.Text}");


        var saved = await _facade.ShareBytesAsync(_session!.SessionId);
        if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);
        await OpenTextStyleSmokeAsync(saved.Value!);
        SetContentEditMode(true);
        await RequireLookAsync("Hello", bold: false, italic: false, underline: true);
        await RequireLookAsync(" world", bold: false, italic: true, underline: true);

        // Typed, then toggled before the typing pause runs out: the text survives.
        SelectTextBlock(await StyleBlockAsync("Hello world"));
        await RetypeSelectedTextBlockAsync();
        var typing = _pump.Box ?? throw new Exception("No editor over the block.");
        typing.Box.Text = "Hello world!";
        typing.Box.Select(typing.Box.Text.Length, 0);
        await ToggleTextStyleAsync(TextStyleToggle.Bold);
        var typed = (await _facade.PageContentAsync(_session!.SessionId, 0)).Value!.TextRuns.FirstOrDefault(run => run.Text == "Hello world!")
            ?? throw new Exception($"Text typed just before a toggle was lost. Status: {AnnotationStatus.Text}");
        if ((await _facade.TextStyleButtonsAsync(_session.SessionId, [typed])).Value?.Look.Bold != true)
            throw new Exception($"The typed text was not restyled. Status: {AnnotationStatus.Text}");
        await CommitContentEditorAsync();
    }

    private async Task<ContentTextBlock> StyleBlockAsync(string text) =>
        (await _facade.PageContentAsync(_session!.SessionId, 0)).Value!.TextBlocks.FirstOrDefault(block => block.Text == text)
        ?? throw new Exception($"No block {text}.");

    private async Task RequireLookAsync(string text, bool bold, bool italic, bool underline)
    {
        var run = (await _facade.PageContentAsync(_session!.SessionId, 0)).Value!.TextRuns.FirstOrDefault(candidate => candidate.Text == text)
            ?? throw new Exception($"No run {text}.");
        var look = (await _facade.TextStyleButtonsAsync(_session.SessionId, [run])).Value?.Look
            ?? throw new Exception($"The look of {text} cannot be read.");
        if (look.Bold != bold || look.Italic != italic || look.Underline != underline)
            throw new Exception($"{text} looks {look}, not bold={bold} italic={italic} underline={underline}. Status: {AnnotationStatus.Text}");
    }

    /// <summary>Lets the buttons' asynchronous read land.</summary>
    private async Task SettleTogglesAsync()
    {
        UpdateTextStyleToggles();
        await Task.Delay(300);
    }

    private async Task OpenTextStyleSmokeAsync(byte[] bytes)
    {
        var opened = await _facade.OpenAsync(new DocumentSource("Text style smoke", bytes), discardPendingEdits: true);
        if (!opened.IsSuccess) throw new Exception(opened.Error!.Message);
        ShowOpenedDocument(opened.Value!);
        await RefreshAnnotationStateAsync();
        for (var attempt = 0; _slots.Count == 0 && attempt < 100; attempt++) await Task.Delay(50);
        if (_slots.Count == 0) throw new Exception("Pages were not laid out.");
    }

    /// <summary>"Hello world" in Helvetica beside an unused Helvetica-Bold, and "Heavy" in a bold face alone.</summary>
    private static byte[] StyledPdf()
    {
        const string content = "BT /F1 12 Tf 72 700 Td (Hello) Tj ( world) Tj ET BT /F3 12 Tf 72 600 Td (Heavy) Tj ET";
        string[] objects =
        [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R /F3 7 0 R >> >> >>",
            $"<< /Length {content.Length} >>\nstream\n{content}\nendstream",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Courier-Bold /Encoding /WinAnsiEncoding >>",
        ];
        var pdf = new StringBuilder("%PDF-1.7\n");
        var offsets = new List<int>();
        for (var index = 0; index < objects.Length; index++)
        {
            offsets.Add(pdf.Length);
            pdf.Append($"{index + 1} 0 obj\n{objects[index]}\nendobj\n");
        }
        var xref = pdf.Length;
        pdf.Append($"xref\n0 {objects.Length + 1}\n0000000000 65535 f \n");
        foreach (var offset in offsets) pdf.Append($"{offset:D10} 00000 n \n");
        pdf.Append($"trailer\n<< /Size {objects.Length + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n");
        return Encoding.ASCII.GetBytes(pdf.ToString());
    }
}
