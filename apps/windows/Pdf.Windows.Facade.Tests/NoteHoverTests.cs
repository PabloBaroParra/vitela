using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

internal static class NoteHoverTests
{
    public static Task RunAsync()
    {
        var rect = new AnnotationRect(10, 10, 40, 20);
        Annotation Make(AnnotationKind kind, string? contents) => new(1, 0, kind, rect, null, [], contents);

        Check(NoteHover.TextFor(Make(AnnotationKind.TextNote, "Call back Monday")) == "Call back Monday", "a note hovers its own text");
        Check(NoteHover.TextFor(Make(AnnotationKind.TextNote, "  A\rB  ")) == "  A\rB  ", "the text is shown as the core stored it");
        Check(NoteHover.TextFor(Make(AnnotationKind.TextNote, null)) is null, "a note without contents has nothing to show");
        Check(NoteHover.TextFor(Make(AnnotationKind.TextNote, "")) is null, "an empty note has nothing to show");
        Check(NoteHover.TextFor(Make(AnnotationKind.TextNote, " \r\n\t ")) is null, "a whitespace-only note has nothing to show");
        Check(NoteHover.TextFor(Make(AnnotationKind.Highlight, "stray")) is null, "only notes hover text");
        Check(NoteHover.TextFor(Make(AnnotationKind.Stamp, "stray")) is null, "a stamp never hovers text");

        Check(NoteHover.MayShow(organizing: false, busy: false, dialogOpen: false, dragging: false), "an idle viewer shows the tooltip");
        Check(!NoteHover.MayShow(organizing: true, busy: false, dialogOpen: false, dragging: false), "no tooltip while organizing");
        Check(!NoteHover.MayShow(organizing: false, busy: true, dialogOpen: false, dragging: false), "no tooltip while busy");
        Check(!NoteHover.MayShow(organizing: false, busy: false, dialogOpen: true, dragging: false), "no tooltip behind a dialog");
        Check(!NoteHover.MayShow(organizing: false, busy: false, dialogOpen: false, dragging: true), "no tooltip during a drag");
        return Task.CompletedTask;
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
