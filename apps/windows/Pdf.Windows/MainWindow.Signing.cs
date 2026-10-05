using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>Native signing presentation; all certificate and document operations stay in Facade.</summary>
public sealed partial class MainWindow
{
    private readonly Button _chooseSigningCertificate = new();
    private readonly Button _chooseCardCertificate = new();
    private readonly Button _chooseComputerCertificate = new();
    private readonly TextBlock _signedIndicator = new() { Text = "✓ This document is digitally signed.", TextWrapping = TextWrapping.Wrap, Visibility = Visibility.Collapsed };
    private SigningState? _signingState;
    private long _signingGeneration;
    private bool _signingFlowActive;

    private void BuildSigningPanel()
    {
        var panel = new StackPanel { Spacing = 8 };
        AutomationProperties.SetAutomationId(panel, "SigningPanel");
        panel.Children.Add(new TextBlock { Text = "Signing", Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"] });
        AutomationProperties.SetAutomationId(_signedIndicator, "SignedIndicator");
        panel.Children.Add(_signedIndicator);
        _chooseSigningCertificate.Content = new TextBlock { Text = "Choose signing certificate (.pfx)…", TextWrapping = TextWrapping.Wrap };
        _chooseSigningCertificate.HorizontalAlignment = HorizontalAlignment.Stretch;
        AutomationProperties.SetName(_chooseSigningCertificate, "Choose signing certificate (.pfx)…");
        AutomationProperties.SetAutomationId(_chooseSigningCertificate, "ChooseSigningCertificate");
        _chooseSigningCertificate.Click += ChooseSigningCertificate_Click;
        panel.Children.Add(_chooseSigningCertificate);
        foreach (var (button, label, id) in new[] { (_chooseCardCertificate, "Use card or token…", "ChooseCardCertificate"), (_chooseComputerCertificate, "Use a certificate from this computer…", "ChooseComputerCertificate") })
        {
            button.Content = new TextBlock { Text = label, TextWrapping = TextWrapping.Wrap };
            button.HorizontalAlignment = HorizontalAlignment.Stretch;
            AutomationProperties.SetName(button, label);
            AutomationProperties.SetAutomationId(button, id);
            button.Click += ChooseSigningCertificate_Click;
            panel.Children.Add(button);
        }
        _toolPages["Sign"].Children.Insert(0, panel);
        UpdateSigningControls();
    }

    private void UpdateSigningControls()
    {
        var current = _session is not null && _signingState?.SessionId == _session.SessionId;
        _chooseSigningCertificate.IsEnabled = current && _session!.PageCount > 0 && _signingState!.Refusal is null && !_isBusy && !_organizing && !_signingFlowActive;
        _chooseCardCertificate.IsEnabled = _chooseSigningCertificate.IsEnabled;
        _chooseComputerCertificate.IsEnabled = _chooseSigningCertificate.IsEnabled;
        _signedIndicator.Visibility = current && _signingState!.IsSigned ? Visibility.Visible : Visibility.Collapsed;
        ToolTipService.SetToolTip(_chooseSigningCertificate, current && _signingState!.Refusal is { } refusal ? refusal : "Choose a PKCS#12 signing certificate (.pfx or .p12).");
    }

    private async Task RefreshSigningStateAsync()
    {
        var generation = ++_signingGeneration;
        var sessionId = _session?.SessionId;
        _signingState = null;
        UpdateSigningControls();
        if (sessionId is null) return;
        var result = await _facade.SigningStateAsync(sessionId);
        if (_session?.SessionId != sessionId || generation != _signingGeneration) return;
        if (result.IsSuccess) _signingState = result.Value;
        else AnnotationStatus.Text = result.Error!.Message;
        UpdateSigningControls();
    }

    private async void ChooseSigningCertificate_Click(object sender, RoutedEventArgs e)
    {
        if (_isBusy || _dialogOpen || _signingFlowActive || !_chooseSigningCertificate.IsEnabled || _session is null) return;
        var sessionId = _session.SessionId;
        var displayName = _session.DisplayName;
        _signingFlowActive = true;
        SetBusy(true);
        StorageFile? temporary = null;
        try
        {
            if (!await PrepareDocumentLifecycleAsync()) return;
            if (_session?.SessionId != sessionId) return;
            SetBusy(true);
            using var certificate = await ChooseSigningSourceAsync(sender);
            if (certificate is null || _session?.SessionId != sessionId) return;
            var identities = certificate.Identities;
            if (identities.Count == 0)
            {
                AnnotationStatus.Text = "This certificate source has no usable signing identities.";
                return;
            }
            var selected = await AskSigningIdentityAsync(identities);
            if (selected is null || _session?.SessionId != sessionId) return;
            var destination = new FileSavePicker { SuggestedFileName = $"{Path.GetFileNameWithoutExtension(displayName)}-signed" };
            destination.FileTypeChoices.Add("PDF", [".pdf"]);
            InitializeWithWindow.Initialize(destination, WindowNative.GetWindowHandle(this));
            var output = await destination.PickSaveFileAsync();
            if (output is null) { AnnotationStatus.Text = "Signing cancelled."; return; }
            if (_session?.SessionId != sessionId) return;
            AnnotationStatus.Text = "Signing document…";
            var result = await _facade.SignToDestinationAsync(sessionId, certificate, selected.Id, output.Name, async signedBytes =>
            {
                var folder = await StorageFolder.GetFolderFromPathAsync(Path.GetDirectoryName(output.Path)!);
                temporary = await folder.CreateFileAsync($".{output.Name}.{Guid.NewGuid():N}.tmp", CreationCollisionOption.GenerateUniqueName);
                await FileIO.WriteBytesAsync(temporary, signedBytes);
                await temporary.MoveAndReplaceAsync(output);
                temporary = null;
            });
            if (!result.IsSuccess) { AnnotationStatus.Text = result.Error!.Message; return; }
            ShowOpenedDocument(result.Value!);
            SelectToolPage("Sign");
            AnnotationStatus.Text = "Signed PDF saved and reopened.";
        }
        catch (Exception error) { AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message; }
        finally
        {
            if (temporary is not null) { try { await temporary.DeleteAsync(); } catch { } }
            _signingFlowActive = false;
            SetBusy(false);
            await RefreshSigningStateAsync();
        }
    }

}
