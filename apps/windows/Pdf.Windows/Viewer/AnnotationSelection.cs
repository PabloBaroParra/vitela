using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>Navigation through the document's annotations without editing them.</summary>
public static class AnnotationSelection
{
    public static ulong? PreviousId(IReadOnlyList<Annotation> annotations, ulong? selectedId)
        => StepId(annotations, selectedId, forward: false);

    public static ulong? NextId(IReadOnlyList<Annotation> annotations, ulong? selectedId)
        => StepId(annotations, selectedId, forward: true);

    private static ulong? StepId(IReadOnlyList<Annotation> annotations, ulong? selectedId, bool forward)
    {
        if (annotations.Count == 0) return null;
        if (selectedId is null) return forward ? annotations[0].Id : annotations[^1].Id;

        for (var index = 0; index < annotations.Count; index++)
        {
            if (annotations[index].Id == selectedId)
                return annotations[(index + (forward ? 1 : annotations.Count - 1)) % annotations.Count].Id;
        }

        return null;
    }
}
