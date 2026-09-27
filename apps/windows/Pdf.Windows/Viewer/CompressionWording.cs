using System.Globalization;
using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>
/// Every sentence the Compress command shows, as plain functions with no
/// control in sight — the same split the GTK shell makes in
/// <c>write/compress/options.rs</c>, and worded the same way so the two
/// shells report one compression in one vocabulary.
/// </summary>
/// <remarks>
/// <para>
/// <c>BeforeBytes</c> is written "uncompressed", never "the original": it is
/// the size of an uncompressed save of the session, which is not the file on
/// disk once anything has been edited.
/// </para>
/// <para>
/// Nothing here predicts a saving. The core cannot answer that without
/// running, so a number before the run would be a guess printed in the same
/// typeface as a measurement.
/// </para>
/// </remarks>
internal static class CompressionWording
{
    private const double Kilo = 1_000;
    private const double Mega = 1_000_000;

    public static (string Name, string Description) Describe(CompressionPreset preset) => preset switch
    {
        CompressionPreset.Lossless => ("Lossless", "Repack the file only. Every page renders exactly as it does now."),
        CompressionPreset.Balanced => ("Balanced", "Also bring images down to 150 DPI. Visibly unchanged on screen, materially smaller on disk."),
        CompressionPreset.Small => ("Small", "Also bring images down to 96 DPI. For when the file has to fit through something and the pixels matter less than that."),
        _ => throw new ArgumentOutOfRangeException(nameof(preset)),
    };

    /// <summary>Powers of ten, the way file managers on this platform's peers count.</summary>
    public static string HumanSize(ulong bytes)
    {
        var size = (double)bytes;
        if (size >= Mega) return (size / Mega).ToString("0.0", CultureInfo.InvariantCulture) + " MB";
        if (size >= Kilo) return (size / Kilo).ToString("0", CultureInfo.InvariantCulture) + " kB";
        return bytes == 1 ? "1 byte" : $"{bytes} bytes";
    }

    /// <summary>
    /// Truncated rather than rounded, so the number shown is one the file
    /// actually reached: 49.7% smaller reads 49%, never 50%.
    /// </summary>
    public static ulong PercentSmaller(CompressionResult result) =>
        result.BeforeBytes == 0 ? 0 : (ulong)((UInt128)result.SavedBytes * 100 / result.BeforeBytes);

    /// <summary>What the status line says once smaller bytes exist and only the destination is left to ask.</summary>
    public static string ReductionSummary(CompressionResult result) =>
        $"{HumanSize(result.BeforeBytes)} uncompressed → {HumanSize(result.AfterBytes)} compressed ({PercentSmaller(result)}% smaller). Choose where to write it.{RefusalClause(result)}";

    /// <summary>
    /// What it says when nothing smaller came out. No destination is asked
    /// for: the bytes on offer are the ones an ordinary Save would write, and
    /// a picker here would invite filing a copy under a name that promises it
    /// is smaller.
    /// </summary>
    public static string NoGainSummary(CompressionResult result) =>
        $"Nothing to gain: this document compresses to the same {HumanSize(result.BeforeBytes)} an uncompressed save writes, so no file was written.{RefusalClause(result)}";

    public static string WrittenSummary(CompressionResult result, string destination) =>
        $"Compressed PDF written to {destination} ({HumanSize(result.BeforeBytes)} → {HumanSize(result.AfterBytes)}, {PercentSmaller(result)}% smaller).";

    /// <summary>
    /// Both known refusals are gated before the run, so in practice this is
    /// empty — but the core's refusal list is open-ended, and one this shell
    /// did not ask about must still reach the reader instead of vanishing.
    /// </summary>
    private static string RefusalClause(CompressionResult result) =>
        result.Refusals.Count == 0 ? "" : $" Not everything could be done: {string.Join("; ", result.Refusals)}.";
}
