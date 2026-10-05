using Pdf.Windows.Facade;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>Native source selection; private-key access remains in explicit certificate adapters.</summary>
public sealed partial class MainWindow
{
    private async Task<ISigningCertificate?> ChooseSigningSourceAsync(object sender)
    {
        if (ReferenceEquals(sender, _chooseComputerCertificate))
        {
            var result = await _facade.OpenSystemSigningSourceAsync();
            if (!result.IsSuccess) AnnotationStatus.Text = result.Error!.Message;
            return result.Value;
        }

        var token = ReferenceEquals(sender, _chooseCardCertificate);
        var picker = new FileOpenPicker();
        foreach (var extension in token ? new[] { ".dll" } : new[] { ".pfx", ".p12" })
            picker.FileTypeFilter.Add(extension);
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var file = await picker.PickSingleFileAsync();
        if (file is null) { AnnotationStatus.Text = "Certificate selection cancelled."; return null; }
        if (token)
            return await AskTokenSigningCertificateAsync(file.Path);

        var bytes = await File.ReadAllBytesAsync(file.Path);
        try { return await AskSigningCertificateAsync(bytes); }
        finally { Array.Clear(bytes); }
    }
}
