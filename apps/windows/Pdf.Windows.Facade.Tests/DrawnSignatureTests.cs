using System.IO.Compression;
using Pdf.Windows.Viewer;

/// <summary>
/// The drawn signature becomes a PNG cropped to the ink, so the core's stamp
/// placement sizes the signature and not the empty pad around it. Mirrors the
/// Android `DrawnSignatureTest`; the rasterizer is UI-free, so it runs here.
/// </summary>
internal static class DrawnSignatureTests
{
    public static Task RunAsync()
    {
        APadWithoutALineHasNoSignature();
        TheFrameHugsTheInkPlusHalfAStroke();
        ALongSignatureIsScaledToTheCap();
        ASmallSignatureIsNeverEnlarged();
        ThePngIsTransparentBlackInkCroppedToTheFrame();
        TheCapDecidesThePngSize();
        NoLineMeansNoPng();
        return Task.CompletedTask;
    }

    private static List<List<PadPoint>> Strokes(PadPoint[][] strokes) => [.. strokes.Select(s => s.ToList())];

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static void APadWithoutALineHasNoSignature()
    {
        Check(!DrawnSignature.HasInk([]), "an empty pad has no ink");
        Check(!DrawnSignature.HasInk(Strokes([[new(5, 5)]])), "a bare tap leaves no line");
        Check(DrawnSignature.Frame(Strokes([[new(5, 5)]]), 4) is null, "a tap has no frame");
        Check(DrawnSignature.HasInk(Strokes([[new(5, 5), new(9, 9)]])), "two points are a line");
    }

    private static void TheFrameHugsTheInkPlusHalfAStroke()
    {
        var strokes = Strokes([
            [new(100, 50), new(300, 80)],
            [new(5, 5)], // a tap leaves no line, so it does not widen the frame
            [new(150, 120), new(200, 60)],
        ]);

        var frame = DrawnSignature.Frame(strokes, 8) ?? throw new InvalidOperationException("frame expected");

        Check(frame.Left == 96 && frame.Top == 46, $"frame origin was {frame.Left},{frame.Top}");
        Check(frame.Width == 208 && frame.Height == 78, $"frame size was {frame.Width}x{frame.Height}");
        Check(frame.Scale == 1, "a small signature keeps scale 1");
        Check(frame.PixelWidth == 208 && frame.PixelHeight == 78, "pixel size follows the frame");
    }

    private static void ALongSignatureIsScaledToTheCap()
    {
        var frame = DrawnSignature.Frame(Strokes([[new(0, 0), new(2996, 100)]]), 4) ?? throw new InvalidOperationException("frame expected");

        Check(frame.Width == 3000, $"width was {frame.Width}");
        Check(Math.Abs(frame.Scale - 0.4) < 1e-9, $"scale was {frame.Scale}");
        Check(frame.PixelWidth == DrawnSignature.MaxSidePx, $"long side was {frame.PixelWidth}");
        Check(frame.PixelHeight == (int)Math.Ceiling(frame.Height * frame.Scale), "short side follows the scale");
    }

    private static void ASmallSignatureIsNeverEnlarged()
    {
        var frame = DrawnSignature.Frame(Strokes([[new(10, 10), new(20, 10)]]), 2) ?? throw new InvalidOperationException("frame expected");
        Check(frame.Scale == 1, "scale never exceeds 1");
        Check(frame.PixelWidth == 12 && frame.PixelHeight == 2, $"size was {frame.PixelWidth}x{frame.PixelHeight}");
    }

    private static void ThePngIsTransparentBlackInkCroppedToTheFrame()
    {
        var png = SignaturePng.Render(Strokes([[new(10, 10), new(110, 10)]]), 4) ?? throw new InvalidOperationException("png expected");
        var image = DecodedPng.Parse(png);

        Check(image.Width == 104 && image.Height == 4, $"png was {image.Width}x{image.Height}");
        // Mid-stroke is solid black; the corner, past the round cap, is mostly empty.
        var middle = image.Pixel(52, 1);
        Check(middle is { R: 0, G: 0, B: 0, A: 255 }, $"mid-stroke pixel was {middle}");
        Check(image.Pixel(0, 0).A < 128, $"corner alpha was {image.Pixel(0, 0).A}");
        Check(image.Pixel(0, 1).A is > 128 and < 255, "the round cap is anti-aliased, not clipped");
        Check(image.Pixel(0, 0).R == 0 && image.Pixel(0, 0).G == 0 && image.Pixel(0, 0).B == 0, "ink is black even where it is faint");
    }

    private static void TheCapDecidesThePngSize()
    {
        var png = SignaturePng.Render(Strokes([[new(0, 0), new(2996, 100)]]), 4) ?? throw new InvalidOperationException("png expected");
        var image = DecodedPng.Parse(png);
        Check(image.Width == DrawnSignature.MaxSidePx, $"png width was {image.Width}");
        Check(image.Height <= DrawnSignature.MaxSidePx, "png height respects the cap");
    }

    private static void NoLineMeansNoPng()
    {
        Check(SignaturePng.Render(Strokes([[new(5, 5)]]), 4) is null, "a tap renders nothing");
        Check(SignaturePng.Render([], 4) is null, "an empty pad renders nothing");
    }

    /// <summary>Just enough of a PNG reader to inspect an 8-bit RGBA, unfiltered image.</summary>
    private sealed record DecodedPng(int Width, int Height, byte[] Rgba)
    {
        public (byte R, byte G, byte B, byte A) Pixel(int x, int y)
        {
            var at = (y * Width + x) * 4;
            return (Rgba[at], Rgba[at + 1], Rgba[at + 2], Rgba[at + 3]);
        }

        public static DecodedPng Parse(byte[] png)
        {
            byte[] signature = [137, 80, 78, 71, 13, 10, 26, 10];
            Check(png.AsSpan(0, 8).SequenceEqual(signature), "PNG signature");
            int width = 0, height = 0;
            using var idat = new MemoryStream();
            var offset = 8;
            var sawEnd = false;
            while (offset < png.Length)
            {
                var length = ReadInt(png, offset);
                var type = System.Text.Encoding.ASCII.GetString(png, offset + 4, 4);
                var data = png.AsSpan(offset + 8, length);
                var crc = (uint)ReadInt(png, offset + 8 + length);
                Check(crc == Crc(png.AsSpan(offset + 4, 4 + length)), $"{type} chunk CRC");
                switch (type)
                {
                    case "IHDR":
                        width = ReadInt(png, offset + 8);
                        height = ReadInt(png, offset + 12);
                        Check(data[8] == 8 && data[9] == 6 && data[10] == 0 && data[11] == 0 && data[12] == 0, "8-bit RGBA, no interlace");
                        break;
                    case "IDAT": idat.Write(data); break;
                    case "IEND": sawEnd = true; break;
                }
                offset += 12 + length;
            }
            Check(sawEnd, "IEND chunk");
            idat.Position = 0;
            using var inflate = new ZLibStream(idat, CompressionMode.Decompress);
            using var raw = new MemoryStream();
            inflate.CopyTo(raw);
            var bytes = raw.ToArray();
            var stride = width * 4;
            Check(bytes.Length == (stride + 1) * height, "scanline data length");
            var rgba = new byte[stride * height];
            for (var row = 0; row < height; row++)
            {
                Check(bytes[row * (stride + 1)] == 0, "filter type 0");
                Buffer.BlockCopy(bytes, row * (stride + 1) + 1, rgba, row * stride, stride);
            }
            return new DecodedPng(width, height, rgba);
        }

        private static int ReadInt(byte[] bytes, int at) => (bytes[at] << 24) | (bytes[at + 1] << 16) | (bytes[at + 2] << 8) | bytes[at + 3];

        private static uint Crc(ReadOnlySpan<byte> bytes)
        {
            var crc = 0xFFFFFFFFu;
            foreach (var b in bytes)
            {
                crc ^= b;
                for (var bit = 0; bit < 8; bit++) crc = (crc & 1) != 0 ? 0xEDB88320u ^ (crc >> 1) : crc >> 1;
            }
            return ~crc;
        }
    }
}
