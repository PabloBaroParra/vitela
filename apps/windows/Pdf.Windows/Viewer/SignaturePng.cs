using System.IO.Compression;

namespace Pdf.Windows.Viewer;

/// <summary>
/// Turns pad strokes into the PNG the image stamp consumes: black ink with
/// round caps on a transparent background, cropped by
/// <see cref="DrawnSignature.Frame"/>.
/// </summary>
/// <remarks>
/// Rasterized and encoded here rather than through a WinUI render target. The
/// stroke → PNG step is shell-side because the core's stamp builder takes
/// encoded image bytes (as on Android); doing it with no UI type keeps it off
/// the UI thread, independent of what is in the visual tree, and testable. Each
/// pixel's alpha is the coverage of a stroke of the pen's width, so the edge is
/// anti-aliased and the colour channels stay black (straight alpha).
/// </remarks>
public static class SignaturePng
{
    private static readonly byte[] Signature = [137, 80, 78, 71, 13, 10, 26, 10];

    /// <summary>The PNG, or null when the pad holds no line.</summary>
    public static byte[]? Render(IReadOnlyList<IReadOnlyList<PadPoint>> strokes, double strokeWidth)
    {
        if (DrawnSignature.Frame(strokes, strokeWidth) is not { } frame) return null;
        var width = frame.PixelWidth;
        var height = frame.PixelHeight;
        var coverage = new float[width * height];
        var radius = strokeWidth * frame.Scale / 2;
        foreach (var stroke in strokes.Where(stroke => stroke.Count >= 2))
        {
            var path = stroke
                .Select(point => ((point.X - frame.Left) * frame.Scale, (point.Y - frame.Top) * frame.Scale))
                .ToArray();
            for (var i = 1; i < path.Length; i++) Paint(coverage, width, height, path[i - 1], path[i], radius);
        }
        return Encode(coverage, width, height);
    }

    /// <summary>Raises the coverage of every pixel within reach of the segment — a capsule, so both ends are round.</summary>
    private static void Paint(float[] coverage, int width, int height, (double X, double Y) a, (double X, double Y) b, double radius)
    {
        var reach = radius + 1;
        var x0 = Math.Max(0, (int)Math.Floor(Math.Min(a.X, b.X) - reach));
        var x1 = Math.Min(width - 1, (int)Math.Ceiling(Math.Max(a.X, b.X) + reach));
        var y0 = Math.Max(0, (int)Math.Floor(Math.Min(a.Y, b.Y) - reach));
        var y1 = Math.Min(height - 1, (int)Math.Ceiling(Math.Max(a.Y, b.Y) + reach));
        var dx = b.X - a.X;
        var dy = b.Y - a.Y;
        var lengthSquared = dx * dx + dy * dy;
        for (var y = y0; y <= y1; y++)
        {
            for (var x = x0; x <= x1; x++)
            {
                var px = x + 0.5 - a.X;
                var py = y + 0.5 - a.Y;
                var along = lengthSquared == 0 ? 0 : Math.Clamp((px * dx + py * dy) / lengthSquared, 0, 1);
                var distance = Math.Sqrt((px - along * dx) * (px - along * dx) + (py - along * dy) * (py - along * dy));
                var covered = (float)Math.Clamp(radius + 0.5 - distance, 0, 1);
                var at = y * width + x;
                if (covered > coverage[at]) coverage[at] = covered;
            }
        }
    }

    private static byte[] Encode(float[] coverage, int width, int height)
    {
        var stride = width * 4;
        var raw = new byte[(stride + 1) * height];
        for (var y = 0; y < height; y++)
        {
            var row = y * (stride + 1); // filter byte 0, then RGBA with black RGB
            for (var x = 0; x < width; x++) raw[row + 1 + x * 4 + 3] = (byte)Math.Round(coverage[y * width + x] * 255);
        }

        using var compressed = new MemoryStream();
        using (var deflate = new ZLibStream(compressed, CompressionLevel.Optimal, leaveOpen: true)) deflate.Write(raw);

        using var png = new MemoryStream();
        png.Write(Signature);
        var header = new byte[13];
        WriteInt(header, 0, width);
        WriteInt(header, 4, height);
        header[8] = 8; // bit depth
        header[9] = 6; // RGBA
        WriteChunk(png, "IHDR", header);
        WriteChunk(png, "IDAT", compressed.ToArray());
        WriteChunk(png, "IEND", []);
        return png.ToArray();
    }

    private static void WriteChunk(Stream png, string type, byte[] data)
    {
        var body = new byte[4 + data.Length];
        System.Text.Encoding.ASCII.GetBytes(type, body);
        data.CopyTo(body, 4);
        var length = new byte[4];
        WriteInt(length, 0, data.Length);
        png.Write(length);
        png.Write(body);
        var crc = new byte[4];
        WriteInt(crc, 0, unchecked((int)Crc32(body)));
        png.Write(crc);
    }

    private static void WriteInt(byte[] bytes, int at, int value)
    {
        bytes[at] = (byte)(value >> 24);
        bytes[at + 1] = (byte)(value >> 16);
        bytes[at + 2] = (byte)(value >> 8);
        bytes[at + 3] = (byte)value;
    }

    private static uint Crc32(ReadOnlySpan<byte> bytes)
    {
        var crc = 0xFFFFFFFFu;
        foreach (var value in bytes)
        {
            crc ^= value;
            for (var bit = 0; bit < 8; bit++) crc = (crc & 1) != 0 ? 0xEDB88320u ^ (crc >> 1) : crc >> 1;
        }
        return ~crc;
    }
}
