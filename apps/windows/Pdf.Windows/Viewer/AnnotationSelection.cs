using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>Navigation through the document's annotations without editing them.</summary>
public static class AnnotationSelection
{
    public static ulong? PreviousId(IReadOnlyList<Annotation> annotations, ulong? selectedId)
    {
        if (selectedId is null) return null;

        for (var index = 0; index < annotations.Count; index++)
        {
            if (annotations[index].Id == selectedId)
                return annotations[(index + annotations.Count - 1) % annotations.Count].Id;
        }

        return null;
    }
}
