using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>
/// The two insert toggles of the Edit page: arm "Insert text" or "Insert
/// image" and the next click on a page adds brand-new content there instead
/// of targeting whatever already sits under the pointer.
/// </summary>
/// <remarks>
/// Mirrors the GTK shell's <c>content_edit::set_insert_mode</c>. At most one
/// kind is armed, arming either implies content-edit mode itself (a page
/// click while the parent mode is off would look armed and do nothing), and
/// leaving content-edit mode disarms both. A kind stays armed after an
/// insertion — only its own toggle, or leaving the mode, disarms it.
///
/// An existing image still wins a click: pressing one selects it, exactly as
/// it does with nothing armed. Only a click that lands on no image becomes an
/// insertion. The two halves live beside this file — the blank text box in
/// <c>MainWindow.TextInsertion.cs</c>, the picker in
/// <c>MainWindow.ImageInsertion.cs</c>.
/// </remarks>
public sealed partial class MainWindow
{
    private enum ContentInsertKind { Text, Image }

    private ContentInsertKind? _contentInsertKind;

    private void InsertTextButton_Click(object sender, RoutedEventArgs e) =>
        SetContentInsertMode(InsertTextButton.IsChecked == true ? ContentInsertKind.Text : null);

    private void InsertImageButton_Click(object sender, RoutedEventArgs e) =>
        SetContentInsertMode(InsertImageButton.IsChecked == true ? ContentInsertKind.Image : null);

    private void SetContentInsertMode(ContentInsertKind? kind)
    {
        if (kind is not null && !_contentEditMode)
        {
            // Turns the parent mode on synchronously — its arming branch
            // awaits nothing — so the check below sees the result.
            SetContentEditMode(true);
        }

        _contentInsertKind = _contentEditMode ? kind : null;
        SyncContentInsertToggles();
        AnnotationStatus.Text = _contentInsertKind switch
        {
            ContentInsertKind.Text => "Insert text armed — click the page to place a new text box.",
            ContentInsertKind.Image => "Insert image armed — click the page to insert a picture.",
            _ => _contentEditMode ? "Content editing armed — click a text run to retype it." : AnnotationStatus.Text,
        };
    }

    /// <summary>Disarms both kinds without touching the status line.</summary>
    private void ClearContentInsertMode()
    {
        _contentInsertKind = null;
        SyncContentInsertToggles();
    }

    private void SyncContentInsertToggles()
    {
        InsertTextButton.IsChecked = _contentInsertKind == ContentInsertKind.Text;
        InsertImageButton.IsChecked = _contentInsertKind == ContentInsertKind.Image;
    }

    /// <summary>
    /// A click that hit no image while a kind is armed: compose new content at
    /// <paramref name="point"/>.
    /// </summary>
    private Task InsertContentAtAsync(ContentInsertKind kind, uint pageIndex, AnnotationPoint point) => kind switch
    {
        ContentInsertKind.Text => OpenInsertEditorAsync(pageIndex, point),
        _ => InsertContentImageAtAsync(pageIndex, point),
    };
}
