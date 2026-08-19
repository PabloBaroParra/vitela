using System.Security.Cryptography;
using Pdf.Windows.Facade;
using uniffi.pdf_ffi;

// Usage: Pdf.Windows.PackageSmoke <receipt-path>
//
// Renders page one of the sample document the package ships and writes a
// receipt the verification script can assert on. Every field is there to close
// a specific way the package can be wrong:
//
//   pdfium=       which library the core actually loaded. A package that
//                 forgot to stage pdfium.dll still renders on the machine that
//                 built it, through the compile-time vendor path baked into
//                 pdf-render — this line is what tells the two apart.
//   width/height= the page rasterized at all.
//   ink=          it rasterized *content*, not a blank sheet, which is what a
//                 PDFium that silently failed to parse would produce.
//   pixels_sha256 recorded as evidence, deliberately not pinned: unlike the
//                 Linux package job this one has no promise about the host's
//                 font stack, and a hash pinned without that promise would
//                 fail for the wrong reason.
if (args.Length != 1)
{
    Console.Error.WriteLine("usage: Pdf.Windows.PackageSmoke <receipt-path>");
    return 2;
}

try
{
    File.WriteAllText(args[0], RenderSampleReceipt());
    return 0;
}
catch (Exception error)
{
    Console.Error.WriteLine($"package smoke failed: {error.GetType().Name}: {error.Message}");
    return 1;
}

static string RenderSampleReceipt()
{
    BundledPdfium.PointCoreAtBundledLibrary();
    var library = Environment.GetEnvironmentVariable(BundledPdfium.PathVariable);
    // Empty rather than absent is a real case — a shell can export a variable
    // with no value — and it is the locator's "nothing bundled" answer, not a
    // path. Left to the core it becomes a LoadLibrary failure on "", which
    // says nothing about the package being incomplete.
    if (string.IsNullOrWhiteSpace(library))
    {
        throw new InvalidOperationException($"no {BundledPdfium.LibraryFileName} beside {AppContext.BaseDirectory}");
    }

    var samplePath = Path.Combine(AppContext.BaseDirectory, "Assets", "vitela-sample.pdf");
    using var document = PdfFfiMethods.OpenFromBytes(File.ReadAllBytes(samplePath), null);
    using var bitmap = PdfFfiMethods.RenderPage(document, 0, 72, new FfiRenderOptions(false));

    var pixels = bitmap.GetPixels();
    return string.Join('\n',
    [
        $"pdfium={library}",
        $"width={bitmap.Width()}",
        $"height={bitmap.Height()}",
        $"pixels={pixels.Length}",
        $"ink={CountNonWhitePixels(pixels, bitmap.Width(), bitmap.Height(), bitmap.Stride())}",
        $"pixels_sha256={Convert.ToHexStringLower(SHA256.HashData(pixels))}",
        string.Empty,
    ]);
}

// Rows are padded to the renderer's stride, so the trailing bytes of each row
// are not page content and are skipped rather than counted as ink.
static long CountNonWhitePixels(byte[] pixels, uint width, uint height, uint stride)
{
    var ink = 0L;
    for (var row = 0u; row < height; row++)
    {
        var rowStart = row * stride;
        for (var column = 0u; column < width; column++)
        {
            var pixel = rowStart + (column * 4);
            if (pixels[pixel] != 0xFF || pixels[pixel + 1] != 0xFF || pixels[pixel + 2] != 0xFF)
            {
                ink++;
            }
        }
    }

    return ink;
}
