namespace Pdf.Windows.Facade;

/// <summary>
/// A multi-item edit (B25 T-208): the members' own snapshots, which the core
/// records as one undo step.
/// </summary>
internal abstract record PdfCoreContentSelectionEdit(
    IReadOnlyList<PdfCoreContentTextRun> Runs,
    IReadOnlyList<PdfCoreContentImage> Images,
    IReadOnlyList<PdfCoreContentPath> Paths)
{
    public sealed record Move(IReadOnlyList<PdfCoreContentTextRun> Runs, IReadOnlyList<PdfCoreContentImage> Images,
        IReadOnlyList<PdfCoreContentPath> Paths, double Dx, double Dy) : PdfCoreContentSelectionEdit(Runs, Images, Paths);

    public sealed record Remove(IReadOnlyList<PdfCoreContentTextRun> Runs, IReadOnlyList<PdfCoreContentImage> Images,
        IReadOnlyList<PdfCoreContentPath> Paths) : PdfCoreContentSelectionEdit(Runs, Images, Paths);
}

/// <summary>
/// What a marquee or SHIFT+click gathered on one page (B25 T-208): text
/// blocks, images and vector paths, moved and deleted together. A "table" is
/// one of these — its border paths plus the text inside them.
/// </summary>
/// <remarks>
/// Immutable: <see cref="Toggle(ContentTextBlock)"/> and its overloads return
/// a new selection, so a shell holding one never sees it change under a
/// pending await.
/// </remarks>
public sealed class ContentSelection
{
    private ContentSelection(uint pageIndex, IReadOnlyList<ContentTextBlock> blocks, IReadOnlyList<ContentImage> images,
        IReadOnlyList<ContentPath> paths)
        => (PageIndex, Blocks, Images, Paths) = (pageIndex, blocks, images, paths);

    public uint PageIndex { get; }
    public IReadOnlyList<ContentTextBlock> Blocks { get; }
    public IReadOnlyList<ContentImage> Images { get; }
    public IReadOnlyList<ContentPath> Paths { get; }

    public int Count => Blocks.Count + Images.Count + Paths.Count;
    public bool IsEmpty => Count == 0;

    /// <summary>An empty selection on <paramref name="pageIndex"/>, to toggle members into.</summary>
    public static ContentSelection Empty(uint pageIndex) => new(pageIndex, [], [], []);

    /// <summary>
    /// Every item on the page whose box <em>touches</em> <paramref name="area"/>
    /// — intersects it, not only inside it (decided by the user 2026-10-28).
    /// <paramref name="blockBounds"/> is the box the shell draws for a block,
    /// which for a just-edited run is its own estimate.
    /// </summary>
    public static ContentSelection Gather(PageContent content, AnnotationRect area,
        Func<ContentTextBlock, AnnotationRect> blockBounds) =>
        new(content.PageIndex,
            [.. content.TextBlocks.Where(block => Touches(blockBounds(block), area))],
            [.. content.Images.Where(image => Touches(image.Bounds, area))],
            [.. content.Paths.Where(path => Touches(path.Bounds, area))]);

    /// <summary>The union of every member's box, PDF space; <c>null</c> when empty.</summary>
    public AnnotationRect? Bounds(Func<ContentTextBlock, AnnotationRect> blockBounds)
    {
        var boxes = Blocks.Select(blockBounds).Concat(Images.Select(image => image.Bounds)).Concat(Paths.Select(path => path.Bounds)).ToArray();
        if (boxes.Length == 0) return null;
        var left = boxes.Min(box => box.X);
        var bottom = boxes.Min(box => box.Y);
        var right = boxes.Max(box => box.X + box.Width);
        var top = boxes.Max(box => box.Y + box.Height);
        return new AnnotationRect(left, bottom, right - left, top - bottom);
    }

    public bool Contains(ContentTextBlock block) => Blocks.Any(member => member.First.Id == block.First.Id);
    public bool Contains(ContentImage image) => Images.Any(member => member.Id == image.Id);
    public bool Contains(ContentPath path) => Paths.Any(member => member.Id == path.Id);

    public ContentSelection Toggle(ContentTextBlock block) => Contains(block)
        ? new(PageIndex, [.. Blocks.Where(member => member.First.Id != block.First.Id)], Images, Paths)
        : new(PageIndex, [.. Blocks, block], Images, Paths);

    public ContentSelection Toggle(ContentImage image) => Contains(image)
        ? new(PageIndex, Blocks, [.. Images.Where(member => member.Id != image.Id)], Paths)
        : new(PageIndex, Blocks, [.. Images, image], Paths);

    public ContentSelection Toggle(ContentPath path) => Contains(path)
        ? new(PageIndex, Blocks, Images, [.. Paths.Where(member => member.Id != path.Id)])
        : new(PageIndex, Blocks, Images, [.. Paths, path]);

    /// <summary>
    /// The same members as <paramref name="content"/> reads them now, matched
    /// by id — what a moved selection becomes once the page is re-read. A
    /// member the page no longer has drops out.
    /// </summary>
    public ContentSelection Reread(PageContent content) =>
        new(PageIndex,
            [.. content.TextBlocks.Where(block => Blocks.Any(member => block.Runs.Any(run => run.Id == member.First.Id)))],
            [.. content.Images.Where(Contains)],
            [.. content.Paths.Where(Contains)]);

    /// <summary>
    /// Only the members that exist because of a pending edit — what a paste
    /// just added (B25 T-209), told apart from the page's own items by their
    /// synthetic ids.
    /// </summary>
    public ContentSelection OnlyPending() =>
        new(PageIndex,
            [.. Blocks.Where(block => block.Runs.All(run => IsPending(run.Id)))],
            [.. Images.Where(image => IsPending(image.Id))],
            [.. Paths.Where(path => IsPending(path.Id))]);

    private static bool IsPending(ulong id) => id >= PendingIdBase;

    /// <summary>pdf_edit::PENDING_ITEM_ID_BASE.</summary>
    private const ulong PendingIdBase = 1UL << 40;

    internal PdfCoreContentTextRun[] RunSources => [.. Blocks.SelectMany(block => block.Runs).Select(run => run.Source)];
    internal PdfCoreContentImage[] ImageSources => [.. Images.Select(image => image.Source)];
    internal PdfCoreContentPath[] PathSources => [.. Paths.Select(path => path.Source)];

    // Inclusive, so a hairline rule (a box of zero height) is caught by a
    // marquee that crosses it.
    private static bool Touches(AnnotationRect box, AnnotationRect area) =>
        box.X <= area.X + area.Width && area.X <= box.X + box.Width
        && box.Y <= area.Y + area.Height && area.Y <= box.Y + box.Height;
}
