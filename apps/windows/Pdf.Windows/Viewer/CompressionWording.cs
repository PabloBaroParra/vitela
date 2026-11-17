using System.Globalization;
using Pdf.Windows.Facade;
using Pdf.Windows.Localization;

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
        CompressionPreset.Lossless => (I18n.T("compress-preset-lossless"), I18n.T("compress-preset-lossless-description")),
        CompressionPreset.Balanced => (I18n.T("compress-preset-balanced"), I18n.T("compress-preset-balanced-description")),
        CompressionPreset.Small => (I18n.T("compress-preset-small"), I18n.T("compress-preset-small-description")),
        _ => throw new ArgumentOutOfRangeException(nameof(preset)),
    };

    /// <summary>Powers of ten, the way file managers on this platform's peers count.</summary>
    public static string HumanSize(ulong bytes)
    {
        var size = (double)bytes;
        if (size >= Mega)
        {
            // Rounded once, then split, so the decimal separator is the language's.
            var megabytes = (size / Mega).ToString("0.0", CultureInfo.InvariantCulture).Split('.');
            return I18n.T("compress-size-megabytes", ("whole", megabytes[0]), ("tenth", megabytes[1]));
        }
        if (size >= Kilo) return I18n.T("compress-size-kilobytes", ("size", (size / Kilo).ToString("0", CultureInfo.InvariantCulture)));
        return I18n.T("compress-size-bytes", ("count", bytes));
    }

    /// <summary>
    /// Truncated rather than rounded, so the number shown is one the file
    /// actually reached: 49.7% smaller reads 49%, never 50%.
    /// </summary>
    public static ulong PercentSmaller(CompressionResult result) =>
        result.BeforeBytes == 0 ? 0 : (ulong)((UInt128)result.SavedBytes * 100 / result.BeforeBytes);

    /// <summary>What the status line says once smaller bytes exist and only the destination is left to ask.</summary>
    public static string ReductionSummary(CompressionResult result) =>
        I18n.T("compress-reduced", ("before", HumanSize(result.BeforeBytes)), ("after", HumanSize(result.AfterBytes)),
            ("percent", PercentSmaller(result))) + RefusalClause(result);

    /// <summary>
    /// What it says when nothing smaller came out. No destination is asked
    /// for: the bytes on offer are the ones an ordinary Save would write, and
    /// a picker here would invite filing a copy under a name that promises it
    /// is smaller.
    /// </summary>
    public static string NoGainSummary(CompressionResult result) =>
        I18n.T("compress-no-gain", ("size", HumanSize(result.BeforeBytes))) + RefusalClause(result);

    public static string WrittenSummary(CompressionResult result, string destination) =>
        I18n.T("compress-written", ("path", destination), ("before", HumanSize(result.BeforeBytes)),
            ("after", HumanSize(result.AfterBytes)), ("percent", PercentSmaller(result)));

    /// <summary>
    /// Both known refusals are gated before the run, so in practice this is
    /// empty — but the core's refusal list is open-ended, and one this shell
    /// did not ask about must still reach the reader instead of vanishing.
    /// </summary>
    private static string RefusalClause(CompressionResult result) =>
        result.Refusals.Count == 0 ? "" : " " + I18n.T("compress-refusals", ("reasons", string.Join("; ", result.Refusals)));
}
