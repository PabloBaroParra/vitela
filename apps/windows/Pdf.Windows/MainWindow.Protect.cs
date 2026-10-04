using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private bool _protectingDocument;
    private async void ProtectButton_Click(object sender, RoutedEventArgs e)
    {
        if (_protectingDocument || _savingDocument || _dialogOpen || _isBusy || _session is not { } session) return;
        if (!session.ContentEditingAllowed)
        {
            AnnotationStatus.Text = "This document does not permit changes to its protection.";
            return;
        }
        _protectingDocument = true;
        StorageFile? temporary = null;
        try
        {
            SetBusy(true);
            if (!await PrepareDocumentLifecycleAsync()) return;
            SetBusy(false);
            var passwords = await AskProtectionPasswordsAsync();
            if (passwords is null || _session?.SessionId != session.SessionId) return;
            var acknowledged = await AskSignatureLossAsync(protecting: true);
            if (acknowledged is null || _session?.SessionId != session.SessionId) return;
            SetBusy(true);
            var picker = new FileSavePicker { SuggestedFileName = "document", DefaultFileExtension = ".pdf", CommitButtonText = "Save" };
            picker.FileTypeChoices.Add("PDF", [".pdf"]);
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            var file = await picker.PickSaveFileAsync();
            if (file is null)
            {
                AnnotationStatus.Text = "Protection cancelled.";
                return;
            }
            if (_session?.SessionId != session.SessionId) return;
            var result = await _facade.ProtectToDestinationAsync(
                session.SessionId,
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
                acknowledged.Value);
            if (!result.IsSuccess)
            {
                AnnotationStatus.Text = result.Error!.Message;
                return;
            }

            var reopened = await _facade.ReopenProtectedAsync(
                session.SessionId,
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
            _protectingDocument = false;
            RestoreAnnotationControls();
        }
    }

    private async Task<(string Open, string Permissions)?> AskProtectionPasswordsAsync()
    {
        var open = new PasswordBox { Header = "Password to open the document", PasswordRevealMode = PasswordRevealMode.Peek };
        var permissions = new PasswordBox { Header = "Permissions password", PasswordRevealMode = PasswordRevealMode.Peek };
        ToolTipService.SetToolTip(permissions, "Governs what readers may do with the document once it is open, and lifts those restrictions for whoever holds it. It must not be the same as the open password.");
        var error = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Crimson),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(open);
        panel.Children.Add(permissions);
        panel.Children.Add(error);

        var dialog = new ContentDialog
        {
            Title = "Protect with a password",
            Content = panel,
            PrimaryButtonText = "Protect",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };

        dialog.Opened += (_, _) => open.Focus(FocusState.Programmatic);
        open.KeyDown += (_, args) =>
        {
            if (args.Key != global::Windows.System.VirtualKey.Enter) return;
            args.Handled = true;
            permissions.Focus(FocusState.Programmatic);
        };
        dialog.PrimaryButtonClick += (_, args) =>
        {
            error.Text = string.IsNullOrEmpty(open.Password)
                ? "Enter the password the document will ask for when it is opened."
                : string.IsNullOrEmpty(permissions.Password)
                    ? "Enter the permissions password."
                    : open.Password == permissions.Password
                        ? "The two passwords must be different. If they were the same, everyone who can open the document could also change what it permits."
                        : "";
            args.Cancel = error.Text.Length != 0;
            error.Visibility = args.Cancel ? Visibility.Visible : Visibility.Collapsed;
        };
        try
        {
            if (await ShowModalAsync(dialog) == ContentDialogResult.Primary) return (open.Password, permissions.Password);
            AnnotationStatus.Text = "Protection cancelled.";
            return null;
        }
        finally
        {
            open.Password = "";
            permissions.Password = "";
        }
    }
}
