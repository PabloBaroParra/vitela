using System.Globalization;
using Pdf.Windows.Localization;

namespace Pdf.Windows.Facade;

/// <summary>The desktop-friendly PDF metadata format, deliberately without calendar or clock validation.</summary>
public static class MetadataDateText
{
    public static string Format(MetadataDate? date) => date is null ? "" :
        FormattableString.Invariant($"{date.Year:D4}-{date.Month:D2}-{date.Day:D2} {date.Hour:D2}:{date.Minute:D2}:{date.Second:D2}");

    public static MetadataDate? Parse(string raw, MetadataDateOffset offset)
    {
        var text = raw.Trim();
        if (text.Length == 0) return null;
        var split = text.IndexOfAny([' ', '\t', '\r', '\n', '\v', '\f']);
        // Whitespace separates date and time, never individual components.
        if (split < 0) split = text.Length;
        var date = text[..split].Split('-');
        if (date.Length != 3) throw new FormatException(I18n.T("metadata-date-format"));
        var year = Component(date[0], 0, ushort.MaxValue, I18n.T("metadata-year"));
        var month = Component(date[1], 1, 12, I18n.T("metadata-month"));
        var day = Component(date[2], 1, 31, I18n.T("metadata-day"));
        var timeText = text[split..].Trim();
        var time = timeText.Length == 0 ? [] : timeText.Split(':');
        if (time.Length != 0 && time.Length != 2 && time.Length != 3)
            throw new FormatException(I18n.T("metadata-time-format"));
        return new((ushort)year, (byte)month, (byte)day,
            (byte)(time.Length == 0 ? 0 : Component(time[0], 0, 23, I18n.T("metadata-hour"))),
            (byte)(time.Length == 0 ? 0 : Component(time[1], 0, 59, I18n.T("metadata-minute"))),
            (byte)(time.Length < 3 ? 0 : Component(time[2], 0, 59, I18n.T("metadata-second"))), offset);
    }

    private static uint Component(string text, uint min, uint max, string name)
    {
        if (!uint.TryParse(text, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var value))
            throw new FormatException(I18n.T("metadata-component-not-number", ("field", name)));
        if (value < min || value > max) throw new FormatException(I18n.T("metadata-component-range",
            ("field", name), ("value", value.ToString(CultureInfo.InvariantCulture)),
            ("min", min.ToString(CultureInfo.InvariantCulture)), ("max", max.ToString(CultureInfo.InvariantCulture))));
        return value;
    }
}
