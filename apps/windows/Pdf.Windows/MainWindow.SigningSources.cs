using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
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
        {
            var password = new PasswordBox { PasswordRevealMode = PasswordRevealMode.Peek };
            AutomationProperties.SetName(password, "Token PIN");
            var content = new StackPanel { Spacing = 8 };
            content.Children.Add(new TextBlock { Text = "Enter the token PIN, or leave it empty to use the token's own authentication prompt.", TextWrapping = TextWrapping.Wrap });
            content.Children.Add(password);
            var dialog = new ContentDialog { XamlRoot = Content.XamlRoot, Title = "Card or token authentication", Content = content,
                PrimaryButtonText = "Continue", CloseButtonText = "Cancel", DefaultButton = ContentDialogButton.Close };
            try
            {
                if (await ShowModalAsync(dialog) != ContentDialogResult.Primary) { AnnotationStatus.Text = "Certificate selection cancelled."; return null; }
                var pin = password.Password;
                password.Password = "";
                var result = await _facade.OpenTokenSigningSourceAsync(file.Path, pin.Length == 0 ? null : pin);
                if (!result.IsSuccess) AnnotationStatus.Text = result.Error!.Message;
                return result.Value;
            }
            finally { password.Password = ""; }
        }

        var bytes = await File.ReadAllBytesAsync(file.Path);
        try { return await AskSigningCertificateAsync(bytes); }
        finally { Array.Clear(bytes); }
    }
}
