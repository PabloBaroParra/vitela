using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>
/// What a note shows when the pointer rests on it, and when it may. Kept free
/// of WinUI so the rules are testable without a window.
/// </summary>
internal static class NoteHover
{
    /// <summary>The note's own text, or null when this annotation has nothing worth a tooltip.</summary>
    public static string? TextFor(Annotation annotation) =>
        annotation.Kind == AnnotationKind.TextNote && !string.IsNullOrWhiteSpace(annotation.Contents)
            ? annotation.Contents
            : null;

    /// <summary>A tooltip would sit on top of — or fight with — these states, so it stays hidden.</summary>
    public static bool MayShow(bool organizing, bool busy, bool dialogOpen, bool dragging) =>
        !organizing && !busy && !dialogOpen && !dragging;
}
