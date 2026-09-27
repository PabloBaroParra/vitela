namespace Pdf.Windows.Viewer;

/// <summary>
/// Maps a dropdown field's choice to a list position and back, with position
/// 0 reserved for "(none)" — the same shifted layout the GTK shell's
/// <c>forms/fill.rs</c> uses, so "nothing chosen" is a row the reader can
/// pick rather than a state they can only reach by never touching the list.
/// </summary>
internal static class FormFieldChoices
{
    public const string NoChoiceLabel = "(none)";

    /// <summary>
    /// The list position that shows <paramref name="choice"/>. A value outside
    /// <paramref name="options"/> — possible in a file written by another tool
    /// — shows as no choice rather than as a row that is not there.
    /// </summary>
    public static int IndexFor(IReadOnlyList<string> options, string? choice)
    {
        if (choice is null) return 0;
        for (var index = 0; index < options.Count; index++)
        {
            if (options[index] == choice) return index + 1;
        }

        return 0;
    }

    /// <summary>The choice the row at <paramref name="index"/> stands for; <c>null</c> for "(none)" or no row.</summary>
    public static string? ChoiceFor(IReadOnlyList<string> options, int index) =>
        index >= 1 && index <= options.Count ? options[index - 1] : null;
}
