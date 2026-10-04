using System.Globalization;

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
        if (date.Length != 3) throw new FormatException("expected YYYY-MM-DD");
        var year = Component(date[0], 0, ushort.MaxValue, "year");
        var month = Component(date[1], 1, 12, "month");
        var day = Component(date[2], 1, 31, "day");
        var timeText = text[split..].Trim();
        var time = timeText.Length == 0 ? [] : timeText.Split(':');
        if (time.Length != 0 && time.Length != 2 && time.Length != 3)
            throw new FormatException("expected HH:MM or HH:MM:SS");
        return new((ushort)year, (byte)month, (byte)day,
            (byte)(time.Length == 0 ? 0 : Component(time[0], 0, 23, "hour")),
            (byte)(time.Length == 0 ? 0 : Component(time[1], 0, 59, "minute")),
            (byte)(time.Length < 3 ? 0 : Component(time[2], 0, 59, "second")), offset);
    }

    private static uint Component(string text, uint min, uint max, string name)
    {
        if (!uint.TryParse(text, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var value))
            throw new FormatException($"{name} is not a number");
        if (value < min || value > max) throw new FormatException($"{name} {value} is out of range ({min}-{max})");
        return value;
    }
}
