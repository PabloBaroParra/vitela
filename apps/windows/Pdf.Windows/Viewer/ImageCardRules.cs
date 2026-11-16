using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>Which of Replace and Delete the selected canvas image allows.</summary>
public enum ImageControls
{
    /// <summary>No image selected, or nothing may be edited.</summary>
    Nothing,
    /// <summary>Selected, and both commands are available, whatever the image is encoded as.</summary>
    Ready,
    /// <summary>
    /// Selected, it already carries an unsaved edit, and the core may not
    /// write that edit into a new in-memory base: Replace waits for a save.
    /// </summary>
    PendingEdit,
}

/// <summary>
/// The Images card's rules, kept out of the WinUI partial so they can be
/// exercised without a UI runtime — the same reason <see cref="ContentHitTest"/>
/// lives here.
/// </summary>
public static class ImageCardRules
{
    /// <summary>
    /// Mirrors the GTK shell's <c>content_edit::image_controls</c>: an image
    /// with a pending edit is replaceable again wherever the core may
    /// checkpoint, because the core then writes the queue into a new base and
    /// validates the replacement against that. Only a session that may not —
    /// encrypted, signed, holding an imported PDF — still asks for a save.
    /// </summary>
    public static ImageControls Resolve(bool selected, bool pendingEdit, bool canCheckpoint) =>
        !selected ? ImageControls.Nothing
        : pendingEdit && !canCheckpoint ? ImageControls.PendingEdit
        : ImageControls.Ready;

    /// <summary>
    /// The selected image among <paramref name="images"/>, the page as it
    /// reads now: the same id at the same box, or else the one image at that
    /// box. <c>null</c> when none or several are there.
    /// </summary>
    /// <remarks>
    /// A checkpoint renumbers the page it rewrites — an inline image a
    /// replacement wrote as an XObject comes back under another id — so the
    /// id alone cannot follow it. Mirrors the GTK shell's
    /// <c>reselect_after_base_swap</c>, which reselects by page and box.
    /// </remarks>
    public static ContentImage? Current(IReadOnlyList<ContentImage> images, ContentImage selected)
    {
        if (images.FirstOrDefault(image => image.Id == selected.Id && image.Bounds == selected.Bounds) is { } same)
            return same;
        var atBox = images.Where(image => image.Bounds == selected.Bounds).Take(2).ToArray();
        return atBox.Length == 1 ? atBox[0] : null;
    }
}
