using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private async Task<OperationResult<DocumentSession>?> OpenWithPasswordAsync(string displayName, byte[] bytes, bool discardPendingEdits)
    {
        var passwordBox = new PasswordBox { PlaceholderText = "Password", PasswordRevealMode = PasswordRevealMode.Peek };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(passwordBox, "Document password");
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
            Title = "Password required",
            Content = panel,
            PrimaryButtonText = "Open",
            CloseButtonText = "Cancel",
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
            AnnotationStatus.Text = "Opening password-protected PDF...";
            try
            {
                result = await _facade.OpenAsync(new DocumentSource(displayName, bytes), passwordBox.Password, discardPendingEdits);
                if (!result.IsSuccess && result.Error!.RequiresPassword)
                {
                    args.Cancel = true;
                    result = null;
                    errorText.Text = "The password is incorrect. Try again.";
                    errorText.Visibility = Visibility.Visible;
                    AnnotationStatus.Text = "Waiting for the document password.";
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
            AnnotationStatus.Text = "Password entry cancelled.";
            return null;
        }
        finally
        {
            passwordBox.Password = "";
        }
    }
}
