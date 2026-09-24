using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private async void ProtectButton_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null) return;
        var passwords = await AskProtectionPasswordsAsync();
        if (passwords is null) return;

        var signatureQuery = await _facade.ProtectionWillInvalidateSignaturesAsync(_session.SessionId);
        if (!signatureQuery.IsSuccess)
        {
            AnnotationStatus.Text = signatureQuery.Error!.Message;
            return;
        }

        var acknowledged = false;
        if (signatureQuery.Value)
        {
            var warning = new ContentDialog
            {
                Title = "Protecting will break this document's signature",
                Content = new TextBlock
                {
                    Text = "Applying password protection rewrites the file, so its existing digital signature will no longer verify.",
                    TextWrapping = TextWrapping.Wrap,
                },
                PrimaryButtonText = "Protect anyway",
                CloseButtonText = "Cancel",
                DefaultButton = ContentDialogButton.Close,
                XamlRoot = Content.XamlRoot,
            };
            if (await ShowModalAsync(warning) != ContentDialogResult.Primary) return;
            acknowledged = true;
        }

        var picker = new FileSavePicker();
        picker.FileTypeChoices.Add("PDF", [".pdf"]);
        picker.SuggestedFileName = $"{Path.GetFileNameWithoutExtension(_session.DisplayName)}-protected";
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var file = await picker.PickSaveFileAsync();
        if (file is null) return;

        SetBusy(true);
        StorageFile? temporary = null;
        try
        {
            var result = await _facade.ProtectToDestinationAsync(
                _session.SessionId,
                passwords.Value.Open,
                passwords.Value.Permissions,
                async bytes =>
                {
                    var folder = await StorageFolder.GetFolderFromPathAsync(Path.GetDirectoryName(file.Path)!);
                    temporary = await folder.CreateFileAsync($".{file.Name}.{Guid.NewGuid():N}.tmp", CreationCollisionOption.GenerateUniqueName);
                    await FileIO.WriteBytesAsync(temporary, bytes);
                    await temporary.MoveAndReplaceAsync(file);
                    temporary = null;
                },
                acknowledged);
            if (!result.IsSuccess)
            {
                AnnotationStatus.Text = result.Error!.Message;
                return;
            }

            var reopened = await _facade.ReopenProtectedAsync(
                _session.SessionId,
                file.Name,
                result.Value!,
                passwords.Value.Open,
                passwords.Value.Permissions);
            if (!reopened.IsSuccess)
            {
                AnnotationStatus.Text = reopened.Error!.Message;
                return;
            }
            ShowOpenedDocument(reopened.Value!);
            AnnotationStatus.Text = "Protected PDF saved and reopened.";
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
        }
        finally
        {
            if (temporary is not null)
            {
                try { await temporary.DeleteAsync(); }
                catch { }
            }
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }

    private async Task<(string Open, string Permissions)?> AskProtectionPasswordsAsync()
    {
        var open = new PasswordBox { Header = "Password to open the document", PasswordRevealMode = PasswordRevealMode.Peek };
        var permissions = new PasswordBox { Header = "Permissions password", PasswordRevealMode = PasswordRevealMode.Peek };
        var error = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Crimson),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(open);
        panel.Children.Add(permissions);
        panel.Children.Add(new TextBlock
        {
            Text = "Use different passwords. The permissions password controls changes after the document is opened.",
            TextWrapping = TextWrapping.Wrap,
        });
        panel.Children.Add(error);

        var dialog = new ContentDialog
        {
            Title = "Protect with a password",
            Content = panel,
            PrimaryButtonText = "Continue",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };

        while (await ShowModalAsync(dialog) == ContentDialogResult.Primary)
        {
            error.Text = string.IsNullOrEmpty(open.Password)
                ? "Enter the password the document will ask for when it is opened."
                : string.IsNullOrEmpty(permissions.Password)
                    ? "Enter the permissions password."
                    : open.Password == permissions.Password
                        ? "The two passwords must be different."
                        : "";
            if (error.Text.Length == 0) return (open.Password, permissions.Password);
            error.Visibility = Visibility.Visible;
        }
        return null;
    }
}
