namespace Pdf.Windows.Facade;

/// <summary>The three style toggles content-edit mode offers for existing text.</summary>
public enum TextStyleToggle
{
    Bold,
    Italic,
    Underline,
}

/// <summary>
/// How a text run looks to the reader — not how it is painted: a bold run may
/// be a bold face or a stroked outline, an italic one an italic face or a
/// shear. Mirrors the core's <c>TextRunLook</c>.
/// </summary>
public readonly record struct TextRunLook(bool Bold, bool Italic, bool Underline, bool Strike)
{
    public bool Has(TextStyleToggle style) => style switch
    {
        TextStyleToggle.Bold => Bold,
        TextStyleToggle.Italic => Italic,
        _ => Underline,
    };

    public TextRunLook With(TextStyleToggle style, bool on) => style switch
    {
        TextStyleToggle.Bold => this with { Bold = on },
        TextStyleToggle.Italic => this with { Italic = on },
        _ => this with { Underline = on },
    };
}

/// <summary>
/// How one run looks now — a restyle queued but not saved included — and
/// whether bold or italic can come off it. Only a run painted in a bold (or
/// italic) font whose page has no plain sibling cannot: un-bolding is never
/// faked.
/// </summary>
public sealed record TextRunLookState(TextRunLook Look, bool CanRemoveBold, bool CanRemoveItalic);

/// <summary>What the three toggle buttons show for a target.</summary>
/// <param name="Look">The first targeted run's look: what each button shows pressed.</param>
/// <param name="BoldRemovable">
/// Whether pressing Bold may turn it off: false only when the first run is
/// bold and some run's bold is its font's, with no regular sibling.
/// </param>
/// <param name="ItalicRemovable">The same, for Italic.</param>
public sealed record TextStyleButtons(TextRunLook Look, bool BoldRemovable, bool ItalicRemovable)
{
    /// <summary>Whether <paramref name="style"/> shows pressed and cannot be released.</summary>
    public bool IsFixed(TextStyleToggle style) => style switch
    {
        TextStyleToggle.Bold => Look.Bold && !BoldRemovable,
        TextStyleToggle.Italic => Look.Italic && !ItalicRemovable,
        _ => false,
    };

    /// <summary>Why <paramref name="style"/> cannot be released, for its tooltip and the status line.</summary>
    public static string FixedReason(TextStyleToggle style) => style == TextStyleToggle.Bold
        ? "This text's font is bold and the page has no regular version of it, so it cannot be made regular."
        : "This text's font is italic and the page has no upright version of it, so it cannot be made upright.";
}

/// <summary>One run's restyle as the core records it.</summary>
internal sealed record PdfCoreTextRunRestyle(PdfCoreContentTextRun Item, TextRunLook From, TextRunLook To);

/// <summary>
/// What a toggle acts on and records — kept free of the session so it can be
/// tested on its own. A toggle acts on every run of its target and follows the
/// first one: pressing it turns the style on for all of them when the first
/// run lacks it, and off for all of them when it has it. A run already in the
/// wanted state is left out, so a mixed target is evened out rather than
/// flipped run by run.
/// </summary>
internal static class TextStylePolicy
{
    /// <summary>The buttons for <paramref name="states"/>; <c>null</c> for an empty target.</summary>
    public static TextStyleButtons? Buttons(IReadOnlyList<TextRunLookState> states)
    {
        if (states.Count == 0) return null;
        var first = states[0].Look;
        return new TextStyleButtons(
            first,
            !first.Bold || states.All(state => state.CanRemoveBold),
            !first.Italic || states.All(state => state.CanRemoveItalic));
    }

    /// <summary>
    /// The restyles pressing <paramref name="style"/> records, or the reason
    /// it records nothing. Empty only for an empty target.
    /// </summary>
    public static (IReadOnlyList<PdfCoreTextRunRestyle> Restyles, string? Refusal) Restyles(
        IReadOnlyList<(PdfCoreContentTextRun Run, TextRunLookState State)> targets,
        TextStyleToggle style)
    {
        if (Buttons([.. targets.Select(target => target.State)]) is not { } buttons) return ([], null);
        var on = !buttons.Look.Has(style);
        if (!on && buttons.IsFixed(style)) return ([], TextStyleButtons.FixedReason(style));
        return ([.. targets
            .Where(target => target.State.Look.Has(style) != on)
            .Select(target => new PdfCoreTextRunRestyle(target.Run, target.State.Look, target.State.Look.With(style, on)))], null);
    }
}
