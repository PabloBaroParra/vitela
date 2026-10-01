using Microsoft.UI.Xaml;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>Annotation selection and viewport navigation; no document edits.</summary>
public sealed partial class MainWindow
{
    private void PreviousAnnotationButton_Click(object sender, RoutedEventArgs e) => NavigateAnnotation(forward: false);
    private void NextAnnotationButton_Click(object sender, RoutedEventArgs e) => NavigateAnnotation(forward: true);

    private void NavigateAnnotation(bool forward)
    {
        if (_isBusy || _organizing || _session is null || _annotationState is not { } state) return;
        var id = forward
            ? AnnotationSelection.NextId(state.Annotations, _selectedAnnotationId)
            : AnnotationSelection.PreviousId(state.Annotations, _selectedAnnotationId);
        if (id is null) return;
        var annotation = state.Annotations.First(item => item.Id == id);
        if (annotation.PageIndex >= _slots.Count) return;

        _selectedAnnotationId = id;
        AnnotationStatus.Text = $"Selected annotation {id} on page {annotation.PageIndex + 1}.";
        UpdateAnnotationControls(state);
        RedrawAnnotations();

        var pageIndex = (int)annotation.PageIndex;
        var slot = _slots[pageIndex];
        if (AnnotationBounds(annotation) is { } bounds)
        {
            // Use the same rotation-aware placement as the overlay. A page-top
            // jump alone leaves a low or horizontally offscreen annotation hidden.
            var placed = _facade.PlaceRect(bounds, PlacementOf(slot, pageIndex));
            slot.Annotations.StartBringIntoView(new BringIntoViewOptions
            {
                TargetRect = new global::Windows.Foundation.Rect(placed.Left, placed.Top,
                    Math.Max(1, placed.Width), Math.Max(1, placed.Height)),
                AnimationDesired = false,
            });
        }
        else
        {
            slot.Container.StartBringIntoView(new BringIntoViewOptions { AnimationDesired = false });
        }
    }
}
