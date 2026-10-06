using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// <b>Draw signature</b> on the Sign page: a pad, an optional remembered copy on
/// this PC, and arming the image stamp with the result. The next click on a page
/// places it through the same core stamp placement Paste and drops use, which
/// keeps the PNG's proportions. Stroke → PNG is shell-side (the core's stamp
/// builder takes encoded bytes) — see <see cref="SignaturePng"/>.
/// </summary>
public sealed partial class MainWindow
{
    private readonly Button _drawSignatureButton = new();
    private readonly ISignatureStore _signatures = new FileSignatureStore(FileSignatureStore.DefaultPath);

    /// <summary>
    /// The drawn signature waiting for its click, or null. Meaningful only while
    /// <see cref="_armedAnnotation"/> is Stamp: it is dropped as soon as the
    /// tool is anything else, so it can never outlive the arming it belongs to.
    /// </summary>
    private byte[]? _armedStampImage;

    private void BuildDrawSignaturePanel()
    {
        var panel = new StackPanel { Spacing = 8 };
        AutomationProperties.SetAutomationId(panel, "DrawSignaturePanel");
        panel.Children.Add(new TextBlock { Text = "Signature", Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"] });
        _drawSignatureButton.Content = new TextBlock { Text = "Draw signature…", TextWrapping = TextWrapping.Wrap };
        _drawSignatureButton.HorizontalAlignment = HorizontalAlignment.Stretch;
        AutomationProperties.SetName(_drawSignatureButton, "Draw signature");
        AutomationProperties.SetAutomationId(_drawSignatureButton, "DrawSignatureButton");
        ToolTipService.SetToolTip(_drawSignatureButton, "Draw a signature with the mouse, then click a page to place it.");
        _drawSignatureButton.Click += DrawSignatureButton_Click;
        panel.Children.Add(_drawSignatureButton);
        _toolPages["Sign"].Children.Insert(1, panel);
        UpdateDrawSignatureControls();
    }

    /// <summary>Only where a stamp could be placed: the same gate as the Stamp tool.</summary>
    private void UpdateDrawSignatureControls() =>
        _drawSignatureButton.IsEnabled = _annotationState?.EditingAllowed == true && _session is { PageCount: > 0 } && !_organizing && !_isBusy;

    private async void DrawSignatureButton_Click(object sender, RoutedEventArgs e)
    {
        if (_isBusy || _dialogOpen || !_drawSignatureButton.IsEnabled || _session is not { } session) return;
        var sessionId = session.SessionId;
        try
        {
            await DrawSignatureAsync(sessionId);
        }
        catch (Exception)
        {
            ReportDropFailure(sessionId, "The signature could not be drawn.");
        }
    }

    private async Task DrawSignatureAsync(string sessionId)
    {
        var saved = await Task.Run(() => _signatures.Load());
        if (!ImageStampInput.SessionMatches(sessionId, _session?.SessionId)) return;
        if (saved is not null)
        {
            switch (await AskSavedSignatureAsync(saved))
            {
                case SavedSignatureChoice.Use:
                    ArmDrawnSignature(sessionId, saved);
                    return;
                case SavedSignatureChoice.Delete:
                    await Task.Run(() => _signatures.Delete());
                    ReportDropFailure(sessionId, "Your saved signature was deleted from this PC.");
                    return;
                case SavedSignatureChoice.DrawNew:
                    break;
                default:
                    return;
            }
            if (!ImageStampInput.SessionMatches(sessionId, _session?.SessionId)) return;
        }

        if (await AskDrawnSignatureAsync() is not { } drawn) return;
        var png = await Task.Run(() => SignaturePng.Render(drawn.Strokes, drawn.StrokeWidth));
        // A document opened while the PNG was made must not receive it.
        if (!ImageStampInput.SessionMatches(sessionId, _session?.SessionId)) return;
        if (png is null)
        {
            AnnotationStatus.Text = "The signature could not be turned into an image.";
            return;
        }

        ArmDrawnSignature(sessionId, png);
        if (!drawn.Remember) return; // used once: any remembered signature stays as it was
        var kept = await Task.Run(() => _signatures.Save(png));
        if (!kept) ReportDropFailure(sessionId, "Your signature could not be remembered. Click a page to place it.");
    }

    /// <summary>
    /// Arms the Stamp tool with <paramref name="png"/>, releasing whatever else
    /// would take the next page click exactly as choosing another annotation tool
    /// does (<see cref="Arm"/>). Arms nothing when the document it was drawn for
    /// is gone or no longer accepts a stamp.
    /// </summary>
    private void ArmDrawnSignature(string sessionId, byte[] png)
    {
        if (!ImageStampInput.SessionMatches(sessionId, _session?.SessionId)) return;
        if (_annotationState?.EditingAllowed != true || _isBusy || _organizing) return;
        StopPlacingFormField();
        SetContentEditMode(false);
        _armedAnnotation = AnnotationKind.Stamp;
        _armedStampImage = png;
        SyncAnnotationToolButtons();
        AnnotationStatus.Text = "Click a page to place your signature.";
    }

    /// <summary>Leaving the Sign page disarms a signature that was not placed.</summary>
    private void DisarmSignatureOutsideSign(string page)
    {
        if (page == "Sign" || _armedStampImage is null) return;
        _armedAnnotation = null;
        SyncAnnotationToolButtons();
        AnnotationStatus.Text = "Signature disarmed.";
    }

    /// <summary>The click that places the armed signature, at the pressed point and at the core's default size.</summary>
    private async Task PlaceArmedStampAsync(uint pageIndex, AnnotationPoint point, byte[] png)
    {
        if (_session is not { } session || _isBusy || _organizing || _dialogOpen || _annotationState?.EditingAllowed != true) return;
        SetBusy(true);
        try
        {
            if (DefaultStampRect(png, point) is not { } rect) return;
            await InsertStampFromImageBytesAsync(session.SessionId, pageIndex, rect, png);
        }
        finally
        {
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }
}
