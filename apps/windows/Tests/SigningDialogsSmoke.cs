using System.Security.Cryptography;
using System.Security.Cryptography.X509Certificates;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

public partial class App : Application
{
    private MainWindow? _window;
    public App() => BundledPdfium.PointCoreAtBundledLibrary();
    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var output = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")
            ?? throw new InvalidOperationException("Set VITELA_SMOKE_OUTPUT to an existing directory.");
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.SigningDialogsSmokeAsync();
            File.WriteAllText(Path.Combine(output, "signing-dialogs-smoke.log"),
                "PASS ephemeral PFX wrong password/same-modal retry/cleared secret/real unlock and Cancel; identity labels/radio exclusivity/default/empty correction/Sign/Cancel; token PIN Cancel/default/secret clearing and invalid-module refusal with empty/nonempty PIN.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "signing-dialogs-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task SigningDialogsSmokeAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        static IEnumerable<DependencyObject> Descendants(DependencyObject parent)
        {
            yield return parent;
            for (var i = 0; i < VisualTreeHelper.GetChildrenCount(parent); i++)
                foreach (var child in Descendants(VisualTreeHelper.GetChild(parent, i))) yield return child;
        }
        async Task Wait(Func<bool> predicate, string message)
        {
            for (var i = 0; i < 200 && !predicate(); i++) await Task.Delay(25);
            Check(predicate(), message);
        }
        ContentDialog? CurrentDialog() => Content.XamlRoot is { } root
            ? VisualTreeHelper.GetOpenPopupsForXamlRoot(root).SelectMany(p => Descendants(p.Child)).OfType<ContentDialog>().FirstOrDefault() : null;
        async Task<ContentDialog> Dialog(string title)
        {
            await Wait(() => CurrentDialog()?.Title?.ToString() == title, "Missing " + title);
            var dialog = CurrentDialog()!;
            await Wait(() => Descendants(dialog).OfType<Button>().Any(b => b.Name == "PrimaryButton"), "Missing buttons");
            return dialog;
        }
        static void Click(ContentDialog dialog, string name)
        {
            var button = Descendants(dialog).OfType<Button>().Single(b => b.Name == name);
            ((IInvokeProvider)FrameworkElementAutomationPeer.CreatePeerForElement(button).GetPattern(PatternInterface.Invoke)).Invoke();
        }
        await Wait(() => Content.XamlRoot is not null, "Window not loaded");
        using var rsa = RSA.Create(2048);
        var request = new CertificateRequest("CN=Vitela dialog smoke", rsa, HashAlgorithmName.SHA256, RSASignaturePadding.Pkcs1);
        using var identity = request.CreateSelfSigned(DateTimeOffset.UtcNow.AddDays(-1), DateTimeOffset.UtcNow.AddDays(1));
        var bytes = identity.Export(X509ContentType.Pfx, "smoke-only");
        try
        {
            var unlocking = AskSigningCertificateAsync(bytes);
            var dialog = await Dialog("Certificate password required");
            var panel = (StackPanel)dialog.Content;
            var password = (PasswordBox)panel.Children[0];
            var error = (TextBlock)panel.Children[1];
            Check(dialog.PrimaryButtonText == "Unlock" && dialog.CloseButtonText == "Cancel", "Unlock actions");
            password.Password = "wrong";
            Click(dialog, "PrimaryButton");
            await Wait(() => error.Text.Length > 0 && dialog.IsPrimaryButtonEnabled, "Wrong password retry");
            Check(!unlocking.IsCompleted && CurrentDialog() == dialog && password.Password == "", "Retry must retain modal and clear secret");
            Check(error.Text == "The password is incorrect, or the file is not a valid PKCS#12 certificate.", "Wrong-password wording");
            password.Password = "smoke-only";
            Click(dialog, "PrimaryButton");
            using var certificate = await unlocking;
            Check(certificate?.Identities.Count == 1 && password.Password == "", "Real unlock/secret clearing");
            unlocking = AskSigningCertificateAsync(bytes);
            dialog = await Dialog("Certificate password required");
            password = (PasswordBox)((StackPanel)dialog.Content).Children[0];
            password.Password = "secret to clear";
            Click(dialog, "CloseButton");
            Check(await unlocking is null && password.Password == "", "Certificate Cancel/secret clearing");

            var identities = new[] { new SigningIdentity("first", "First identity"), new SigningIdentity("second", "Second identity") };
            var choosing = AskSigningIdentityAsync(identities);
            dialog = await Dialog("Choose a signing identity");
            panel = (StackPanel)dialog.Content;
            var choices = (RadioButtons)panel.Children[1];
            Check(((TextBlock)panel.Children[0]).Text == "Sign this document with:" && choices.SelectedIndex == 0, "Identity label/first default");
            Check(dialog.DefaultButton == ContentDialogButton.Close, "Identity Cancel default");
            choices.SelectedIndex = -1;
            Click(dialog, "PrimaryButton");
            await Wait(() => ((TextBlock)panel.Children[2]).Text.Length > 0, "Missing identity validation");
            Check(!choosing.IsCompleted && CurrentDialog() == dialog, "Identity invalid choice must retain modal");
            choices.SelectedIndex = 1;
            Click(dialog, "PrimaryButton");
            Check(await choosing == identities[1], "Chosen identity mapping");
            choosing = AskSigningIdentityAsync(identities);
            dialog = await Dialog("Choose a signing identity");
            Click(dialog, "CloseButton");
            Check(await choosing is null && AnnotationStatus.Text == "Signing cancelled.", "Identity Cancel");

            var missingModule = Path.Combine(Path.GetTempPath(), $"vitela-missing-token-{Guid.NewGuid():N}.dll");
            var authenticating = AskTokenSigningCertificateAsync(missingModule);
            dialog = await Dialog("Card or token authentication");
            panel = (StackPanel)dialog.Content;
            password = (PasswordBox)panel.Children[1];
            Check(dialog.PrimaryButtonText == "Continue" && dialog.CloseButtonText == "Cancel" &&
                dialog.DefaultButton == ContentDialogButton.Close, "Token actions/Cancel default");
            Check(((TextBlock)panel.Children[0]).Text == "Enter the token PIN, or leave it empty to use the token's own authentication prompt.", "Token authentication guidance");
            password.Password = "secret to clear";
            Click(dialog, "CloseButton");
            Check(await authenticating is null && password.Password == "" &&
                AnnotationStatus.Text == "Certificate selection cancelled.", "Token Cancel/secret clearing");
            foreach (var pin in new[] { "", "smoke-only" })
            {
                authenticating = AskTokenSigningCertificateAsync(missingModule);
                dialog = await Dialog("Card or token authentication");
                password = (PasswordBox)((StackPanel)dialog.Content).Children[1];
                password.Password = pin;
                AnnotationStatus.Text = "";
                Click(dialog, "PrimaryButton");
                Check(await authenticating is null && password.Password == "" && AnnotationStatus.Text.Length > 0 &&
                    AnnotationStatus.Text != "Certificate selection cancelled.", "Invalid token module must refuse and clear PIN");
            }
        }
        finally { Array.Clear(bytes); }
    }
}
