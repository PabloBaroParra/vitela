using uniffi.pdf_ffi;

namespace Pdf.Windows.Facade;

internal sealed partial class GeneratedPdfCore
{
    private static MetadataDate? MetadataDateFromCore(FfiPdfDate? date) => date is null ? null :
        new(date.Year, date.Month, date.Day, date.Hour, date.Minute, date.Second, date.Offset switch
        {
            FfiPdfDateOffset.Plus plus => new(1, plus.Hours, plus.Minutes),
            FfiPdfDateOffset.Minus minus => new(-1, minus.Hours, minus.Minutes),
            _ => new(),
        });

    private static FfiPdfDate? MetadataDateToCore(object? value) => value is MetadataDate date ?
        new(date.Year, date.Month, date.Day, date.Hour, date.Minute, date.Second, date.Offset.Sign switch
        {
            > 0 => new FfiPdfDateOffset.Plus(date.Offset.Hours, date.Offset.Minutes),
            < 0 => new FfiPdfDateOffset.Minus(date.Offset.Hours, date.Offset.Minutes),
            _ => new FfiPdfDateOffset.Utc(),
        }) : value is null ? null : throw new InvalidOperationException("Unexpected metadata date representation.");
}
