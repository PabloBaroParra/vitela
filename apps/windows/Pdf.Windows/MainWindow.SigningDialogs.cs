using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Pdf.Windows.Localization;

namespace Pdf.Windows;

/// <summary>Certificate unlock and identity selection, separate from output orchestration.</summary>
public sealed partial class MainWindow
{
    private async Task<ISigningCertificate?> AskTokenSigningCertificateAsync(string modulePath)
    {
        var password = new PasswordBox { PasswordRevealMode = PasswordRevealMode.Peek };
        AutomationProperties.SetName(password, I18n.T("sign-token-pin-name"));
        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(new TextBlock { Text = I18n.T("sign-token-pin-prompt"), TextWrapping = TextWrapping.Wrap });
        content.Children.Add(password);
        var dialog = new ContentDialog { XamlRoot = Content.XamlRoot, Title = I18n.T("sign-token-title"), Content = content,
            PrimaryButtonText = I18n.T("sign-continue"), CloseButtonText = I18n.T("sign-cancel"), DefaultButton = ContentDialogButton.Close };
        try
        {
            if (await ShowModalAsync(dialog) != ContentDialogResult.Primary) { AnnotationStatus.Text = I18n.T("sign-certificate-cancelled"); return null; }
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
        AutomationProperties.SetName(password, I18n.T("sign-pfx-password-name"));
        var error = new TextBlock { TextWrapping = TextWrapping.Wrap };
        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(password);
        content.Children.Add(error);
        var dialog = new ContentDialog { XamlRoot = Content.XamlRoot, Title = I18n.T("sign-pfx-password-title"), Content = content,
            PrimaryButtonText = I18n.T("sign-unlock"), CloseButtonText = I18n.T("sign-cancel"), DefaultButton = ContentDialogButton.Primary };
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
                AnnotationStatus.Text = I18n.T("sign-pfx-reading");
                var secret = password.Password;
                password.Password = "";
                var result = await _facade.OpenSigningCertificateAsync(bytes, secret);
                if (result.IsSuccess) certificate = result.Value;
                else if (result.Error!.RequiresPassword)
                {
                    args.Cancel = true;
                    error.Text = I18n.T("sign-pfx-wrong-password");
                    AnnotationStatus.Text = I18n.T("sign-pfx-waiting");
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
                AnnotationStatus.Text = I18n.T("sign-certificate-cancelled");
            }
            return certificate;
        }
        finally { password.Password = ""; }
    }

    private async Task<SigningIdentity?> AskSigningIdentityAsync(IReadOnlyList<SigningIdentity> identities)
    {
        var choices = new RadioButtons { ItemsSource = identities.Select(i => i.DisplayName).ToArray(), SelectedIndex = identities.Count > 0 ? 0 : -1 };
        AutomationProperties.SetName(choices, I18n.T("sign-identity-name"));
        var error = new TextBlock { TextWrapping = TextWrapping.Wrap };
        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(new TextBlock { Text = I18n.T("sign-picker-prompt"), TextWrapping = TextWrapping.Wrap });
        content.Children.Add(choices);
        content.Children.Add(error);
        var dialog = new ContentDialog { XamlRoot = Content.XamlRoot, Title = I18n.T("sign-picker-title"), Content = content,
            PrimaryButtonText = I18n.T("sign-picker-sign"), CloseButtonText = I18n.T("sign-cancel"), DefaultButton = ContentDialogButton.Close };
        dialog.PrimaryButtonClick += (_, args) =>
        {
            if (choices.SelectedIndex >= 0 && choices.SelectedIndex < identities.Count) return;
            args.Cancel = true;
            error.Text = I18n.T("sign-picker-none-chosen");
        };
        if (await ShowModalAsync(dialog) == ContentDialogResult.Primary)
            return identities[choices.SelectedIndex];
        AnnotationStatus.Text = I18n.T("sign-cancelled");
        return null;
    }
}
