using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;
using Pdf.Windows.Localization;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private async Task<OperationResult<DocumentSession>?> OpenWithPasswordAsync(string displayName, byte[] bytes, bool discardPendingEdits)
    {
        var passwordBox = new PasswordBox { PlaceholderText = I18n.T("open-password-placeholder"), PasswordRevealMode = PasswordRevealMode.Peek };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(passwordBox, I18n.T("open-password-name"));
        var errorText = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Crimson),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetLiveSetting(errorText, Microsoft.UI.Xaml.Automation.Peers.AutomationLiveSetting.Polite);
        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(passwordBox);
        panel.Children.Add(errorText);
        var dialog = new ContentDialog
        {
            Title = I18n.T("open-password-title"),
            Content = panel,
            PrimaryButtonText = I18n.T("open-password-open"),
            CloseButtonText = I18n.T("open-password-cancel"),
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        OperationResult<DocumentSession>? result = null;
        dialog.Opened += (_, _) => passwordBox.Focus(FocusState.Programmatic);
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            dialog.IsPrimaryButtonEnabled = false;
            dialog.IsEnabled = false;
            SetBusy(true);
            AnnotationStatus.Text = I18n.T("open-password-busy");
            try
            {
                result = await _facade.OpenAsync(new DocumentSource(displayName, bytes), passwordBox.Password, discardPendingEdits);
                if (!result.IsSuccess && result.Error!.RequiresPassword)
                {
                    args.Cancel = true;
                    result = null;
                    errorText.Text = I18n.T("open-password-wrong");
                    errorText.Visibility = Visibility.Visible;
                    AnnotationStatus.Text = I18n.T("open-password-waiting");
                }
            }
            finally
            {
                passwordBox.Password = "";
                SetBusy(false);
                dialog.IsEnabled = true;
                dialog.IsPrimaryButtonEnabled = true;
                if (args.Cancel) passwordBox.Focus(FocusState.Programmatic);
                deferral.Complete();
            }
        };
        try
        {
            if (await ShowModalAsync(dialog) == ContentDialogResult.Primary) return result;
            AnnotationStatus.Text = I18n.T("open-password-cancelled");
            return null;
        }
        finally
        {
            passwordBox.Password = "";
        }
    }
}
