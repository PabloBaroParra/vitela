using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>Placing a text field on a page; the core owns its identity and undo step.</summary>
public sealed partial class MainWindow
{
    private bool _placingTextField;
    private bool _creatingTextField;
    private int? _textFieldPressPage;

    private async void PlaceTextFieldButton_Click(object sender, RoutedEventArgs e)
    {
        var requested = PlaceTextFieldButton.IsChecked == true;
        _placingTextField = false;
        _textFieldPressPage = null;
        if (!requested) return;
        var sessionId = _session?.SessionId;
        if (sessionId is null)
        {
            StopPlacingTextField();
            return;
        }
        await SettleContentEditorForHistoryAsync();
        if (_session?.SessionId != sessionId || PlaceTextFieldButton.IsChecked != true)
        {
            StopPlacingTextField();
            return;
        }
        SetContentEditMode(false);
        _placingTextField = true;
        _armedAnnotation = null;
        FormFieldsPanel.IsExpanded = true;
        FormFieldsStatus.Text = "Click a page to place a text field.";
    }

    private void StopPlacingTextField()
    {
        _placingTextField = false;
        _textFieldPressPage = null;
        PlaceTextFieldButton.IsChecked = false;
    }

    private bool BeginTextFieldPlacement(int pageIndex, PointerRoutedEventArgs args)
    {
        if (!_placingTextField) return false;
        _textFieldPressPage = pageIndex;
        args.Handled = true;
        return true;
    }

    private async Task<bool> EndTextFieldPlacementAsync(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (!_placingTextField) return false;
        if (_textFieldPressPage != pageIndex || _creatingTextField || _session is null) return true;
        _textFieldPressPage = null;

        var sessionId = _session.SessionId;
        var point = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position);
        var page = _session.Pages[pageIndex];
        // A click positions the field; keep the default 144 x 36 pt box on
        // even a small page instead of recording a rectangle outside it.
        var width = Math.Min(144, page.WidthPt);
        var height = Math.Min(36, page.HeightPt);
        if (width <= 0 || height <= 0) return true;
        var rect = new PdfCoreRect(Math.Clamp(point.X, 0, page.WidthPt - width),
            Math.Clamp(point.Y - height, 0, page.HeightPt - height), width, height);

        _creatingTextField = true;
        try
        {
            var result = await _facade.AddTextFieldAsync(sessionId, (uint)pageIndex, rect);
            if (_session?.SessionId != sessionId) return true;
            if (!result.IsSuccess)
            {
                FormFieldsStatus.Text = result.Error!.Message;
                return true;
            }

            _annotationState = result.Value!;
            UpdateAnnotationControls(_annotationState);
            _filledFieldPages.Add((uint)pageIndex);
            InvalidatePageRender((uint)pageIndex);
            await RefreshFormFieldsAsync();
            if (_session?.SessionId != sessionId) return true;
            FormFieldsStatus.Text = "Text field placed. Save to keep the change.";
            StopPlacingTextField();
        }
        finally
        {
            _creatingTextField = false;
        }
        return true;
    }
}
