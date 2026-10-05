using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>Certificate unlock and identity selection, separate from output orchestration.</summary>
public sealed partial class MainWindow
{
    private async Task<ISigningCertificate?> AskTokenSigningCertificateAsync(string modulePath)
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
            var result = await _facade.OpenTokenSigningSourceAsync(modulePath, pin.Length == 0 ? null : pin);
            if (!result.IsSuccess) AnnotationStatus.Text = result.Error!.Message;
            return result.Value;
        }
        finally { password.Password = ""; }
    }

    private async Task<ISigningCertificate?> AskSigningCertificateAsync(byte[] bytes)
    {
        var password = new PasswordBox { PasswordRevealMode = PasswordRevealMode.Peek };
        AutomationProperties.SetName(password, "Certificate password");
        var error = new TextBlock { TextWrapping = TextWrapping.Wrap };
        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(password);
        content.Children.Add(error);
        var dialog = new ContentDialog { XamlRoot = Content.XamlRoot, Title = "Certificate password required", Content = content,
            PrimaryButtonText = "Unlock", CloseButtonText = "Cancel", DefaultButton = ContentDialogButton.Primary };
        ISigningCertificate? certificate = null;
        dialog.Opened += (_, _) => password.Focus(FocusState.Programmatic);
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            dialog.IsPrimaryButtonEnabled = false;
            dialog.IsSecondaryButtonEnabled = false;
            password.IsEnabled = false;
            try
            {
                AnnotationStatus.Text = "Reading certificate file...";
                var secret = password.Password;
                password.Password = "";
                var result = await _facade.OpenSigningCertificateAsync(bytes, secret);
                if (result.IsSuccess) certificate = result.Value;
                else if (result.Error!.RequiresPassword)
                {
                    args.Cancel = true;
                    error.Text = "The password is incorrect, or the file is not a valid PKCS#12 certificate.";
                    AnnotationStatus.Text = "Waiting for the certificate password.";
                }
                else AnnotationStatus.Text = result.Error.Message;
            }
            finally
            {
                password.IsEnabled = true;
                dialog.IsPrimaryButtonEnabled = true;
                dialog.IsSecondaryButtonEnabled = true;
                deferral.Complete();
                if (args.Cancel) password.Focus(FocusState.Programmatic);
            }
        };
        try
        {
            if (await ShowModalAsync(dialog) != ContentDialogResult.Primary)
            {
                certificate?.Dispose();
                certificate = null;
                AnnotationStatus.Text = "Certificate selection cancelled.";
            }
            return certificate;
        }
        finally { password.Password = ""; }
    }

    private async Task<SigningIdentity?> AskSigningIdentityAsync(IReadOnlyList<SigningIdentity> identities)
    {
        var choices = new RadioButtons { ItemsSource = identities.Select(i => i.DisplayName).ToArray(), SelectedIndex = identities.Count > 0 ? 0 : -1 };
        AutomationProperties.SetName(choices, "Signing identity");
        var error = new TextBlock { TextWrapping = TextWrapping.Wrap };
        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(new TextBlock { Text = "Sign this document with:", TextWrapping = TextWrapping.Wrap });
        content.Children.Add(choices);
        content.Children.Add(error);
        var dialog = new ContentDialog { XamlRoot = Content.XamlRoot, Title = "Choose a signing identity", Content = content,
            PrimaryButtonText = "Sign", CloseButtonText = "Cancel", DefaultButton = ContentDialogButton.Close };
        dialog.PrimaryButtonClick += (_, args) =>
        {
            if (choices.SelectedIndex >= 0 && choices.SelectedIndex < identities.Count) return;
            args.Cancel = true;
            error.Text = "Choose a signing identity.";
        };
        if (await ShowModalAsync(dialog) == ContentDialogResult.Primary)
            return identities[choices.SelectedIndex];
        AnnotationStatus.Text = "Signing cancelled.";
        return null;
    }
}
