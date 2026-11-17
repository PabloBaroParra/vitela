using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Pdf.Windows.Localization;

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
            ContentInsertKind.Text => I18n.T("edit-insert-text-armed"),
            ContentInsertKind.Image => I18n.T("edit-insert-image-armed"),
            _ => _contentEditMode ? I18n.T("edit-content-editing-armed-run") : AnnotationStatus.Text,
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
        // Armed, every click places something new: nothing on the page is
        // what a press would grab.
        if (_contentInsertKind is not null) ClearContentHover();
        ScheduleQuickActions();
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
