using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Controls.Primitives;
using Pdf.Windows.Facade;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Security.Cryptography.X509Certificates;
using Windows.Storage;

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
            await _window.LifecycleDialogsSmokeAsync();
            File.WriteAllText(Path.Combine(output, "lifecycle-dialogs-smoke.log"),
                "PASS clean signed source Compress-specific warning Cancel/default/ack and real compression, signed Extract completion note/unchanged source; Protect two roles, same-modal empty/equal validation and secret clearing; Protect signed-source shared warning/cancel/default; paused inline text committed, refused text blocks lifecycle and retains editor, corrected text releases refusal; real encrypted sample wrong password/same modal/clear/retry and cancel; real signed source Save warning Cancel/default/acknowledgement; real atomic save, locked-destination failure preserves file and pending work; replacement Cancel/Discard; fresh A4 blank; native close Cancel/Discard and approved programmatic close. Native Save picker cancellation tested separately.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "lifecycle-dialogs-smoke.log"), "FAIL " + error); }
        finally { if (!_window.LifecycleSmokeClosed) _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal bool LifecycleSmokeClosed => _windowClosed;
    [DllImport("user32.dll")]
    private static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);

    internal async Task LifecycleDialogsSmokeAsync()
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
            ? VisualTreeHelper.GetOpenPopupsForXamlRoot(root).SelectMany(popup => Descendants(popup.Child)).OfType<ContentDialog>().FirstOrDefault()
            : null;
        async Task<ContentDialog> Dialog(string title)
        {
            await Wait(() => CurrentDialog()?.Title?.ToString() == title, "Missing modal: " + title);
            var dialog = CurrentDialog()!;
            await Wait(() => Descendants(dialog).OfType<Button>().Any(button => button.Name == "PrimaryButton"), "Modal buttons not loaded");
            return dialog;
        }
        static void Click(ContentDialog dialog, string name)
        {
            var button = Descendants(dialog).OfType<Button>().Single(button => button.Name == name);
            var peer = FrameworkElementAutomationPeer.CreatePeerForElement(button);
            ((IInvokeProvider)peer.GetPattern(PatternInterface.Invoke)).Invoke();
        }

        await Wait(() => Content.XamlRoot is not null, "Window did not load");
        var passwordRequest = AskProtectionPasswordsAsync();
        var protectionDialog = await Dialog("Protect with a password");
        Check(protectionDialog.PrimaryButtonText == "Protect", "Protection action label");
        var protectionPanel = (StackPanel)protectionDialog.Content;
        var fields = protectionPanel.Children.OfType<PasswordBox>().ToArray();
        var validation = protectionPanel.Children.OfType<TextBlock>().Single();
        Check(fields.Length == 2 && fields[0].Header.ToString() == "Password to open the document"
            && fields[1].Header.ToString() == "Permissions password", "Two distinct protection roles must be visible");
        Click(protectionDialog, "PrimaryButton");
        await Wait(() => validation.Text == "Enter the password the document will ask for when it is opened.", "Empty open password validation");
        fields[0].Password = "open-smoke";
        Click(protectionDialog, "PrimaryButton");
        await Wait(() => validation.Text == "Enter the permissions password.", "Empty permissions password validation");
        fields[1].Password = "open-smoke";
        Click(protectionDialog, "PrimaryButton");
        await Wait(() => validation.Text.Contains("everyone who can open the document could also change what it permits"), "Equal-password explanation");
        Check(ReferenceEquals(CurrentDialog(), protectionDialog), "Validation must not close/reopen protection modal");
        fields[1].Password = "owner-smoke";
        Click(protectionDialog, "PrimaryButton");
        var protection = await passwordRequest;
        Check(protection is { Open: "open-smoke", Permissions: "owner-smoke" }
            && fields.All(field => field.Password == ""), "Distinct roles must submit and clear secret controls");
        await OpenDocumentAsync(SampleDisplayName, await File.ReadAllBytesAsync(SamplePath));
        await RefreshAnnotationStateAsync();
        SetContentEditMode(true);
        var targets = await _facade.PageTextEditTargetsAsync(_session!.SessionId, 0);
        var run = targets.Value!.First(target => !target.RequiresFontSubstitution);
        OpenEditorOver(0, run, run.Bounds.X);
        _pump.Box!.Box.Text = run.Text + " lifecycle";
        _liveEdit.Stop();
        Check(await PrepareDocumentLifecycleAsync() && _pump.Box is null, "Lifecycle must commit paused text before closing its editor");
        var changed = await _facade.PageTextEditTargetsAsync(_session.SessionId, 0);
        Check(changed.Value!.Any(target => target.Text.Contains(" lifecycle"))
            && (await _facade.HasUnsavedChangesAsync(_session.SessionId)).Value, "Paused inline text must reach the authoritative document/guard");
        await ApplyHistoryAsync(true);
        targets = await _facade.PageTextEditTargetsAsync(_session.SessionId, 0);
        run = targets.Value!.First(target => !target.RequiresFontSubstitution);
        OpenEditorOver(0, run, run.Bounds.X);
        var refusedBox = _pump.Box!;
        refusedBox.Box.Text = run.Text + "\U0001F680";
        _liveEdit.Stop();
        Check(!await PrepareDocumentLifecycleAsync() && ReferenceEquals(_pump.Box, refusedBox), "A refused inline edit must block lifecycle and stay editable");
        refusedBox.Box.Text = run.Text;
        _pump.Retry(); // Programmatic text assignment does not run the editor's KeyDown retry owner.
        Check(await PrepareDocumentLifecycleAsync(), "Corrected inline text must release lifecycle refusal");
        SetContentEditMode(false);
        // Full Undo is conservatively dirty in the shell. Save the completed
        // text fixture before testing the independent encrypted-open retry.
        var fixturePath = Path.Combine(Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")!, "lifecycle-text-fixture.pdf");
        Check((await _facade.SaveToDestinationAsync(_session.SessionId,
            bytes => File.WriteAllBytesAsync(fixturePath, bytes))).IsSuccess, "Settle text fixture through a real destination write");
        var original = _session!.SessionId;
        var opening = OpenDocumentAsync(Aes128SampleDisplayName, await File.ReadAllBytesAsync(Aes128SamplePath));
        var passwordDialog = await Dialog("Password required");
        var password = ((StackPanel)passwordDialog.Content).Children.OfType<PasswordBox>().Single();
        password.Password = "incorrect";
        Click(passwordDialog, "PrimaryButton");
        await Wait(() => ((StackPanel)passwordDialog.Content).Children.OfType<TextBlock>().Any(text => text.Text == "The password is incorrect. Try again.") && !_isBusy, "Incorrect password was not surfaced");
        Check(ReferenceEquals(CurrentDialog(), passwordDialog) && password.Password == "" && _session!.SessionId == original,
            "Wrong password must preserve the modal and original document and clear the password");
        password.Password = "user-aes-pass";
        Click(passwordDialog, "PrimaryButton");
        await opening;
        Check(_session!.DisplayName == Aes128SampleDisplayName && _session.SessionId != original, "Password retry must unlock the real sample");

        var unlocked = _session.SessionId;
        opening = OpenDocumentAsync(Rc4128SampleDisplayName, await File.ReadAllBytesAsync(Rc4128SamplePath));
        Click(await Dialog("Password required"), "CloseButton");
        await opening;
        Check(_session!.SessionId == unlocked && AnnotationStatus.Text == "Password entry cancelled.", "Password cancellation must keep the session");

        await OpenDocumentAsync(SampleDisplayName, await File.ReadAllBytesAsync(SamplePath));
        using (var rsa = RSA.Create(2048))
        {
            var request = new CertificateRequest("CN=Vitela lifecycle smoke", rsa, HashAlgorithmName.SHA256, RSASignaturePadding.Pkcs1);
            using var testCertificate = request.CreateSelfSigned(DateTimeOffset.UtcNow.AddDays(-1), DateTimeOffset.UtcNow.AddDays(1));
            var pfx = testCertificate.Export(X509ContentType.Pfx, "smoke-only");
            try
            {
                var certificateResult = await _facade.OpenSigningCertificateAsync(pfx, "smoke-only");
                Check(certificateResult.IsSuccess, "PFX fixture failed");
                using var certificate = certificateResult.Value!;
                var signed = await _facade.SignToDestinationAsync(_session!.SessionId, certificate, certificate.Identities[0].Id,
                    "lifecycle-signed.pdf", _ => Task.CompletedTask);
                Check(signed.IsSuccess, "Signed source fixture failed: " + signed.Error?.Message);
                ShowOpenedDocument(signed.Value!);
            }
            finally { Array.Clear(pfx); }
        }
        var protectAcknowledgement = AskSignatureLossAsync(protecting: true);
        var protectSignatureDialog = await Dialog("Saving will break this document's signature");
        Check(protectSignatureDialog.DefaultButton == ContentDialogButton.Close && protectSignatureDialog.PrimaryButtonText == "Save anyway", "Protect uses shared Linux warning/default");
        Click(protectSignatureDialog, "CloseButton");
        Check(await protectAcknowledgement is null && AnnotationStatus.Text == "Protection cancelled.", "Protection signature cancel must keep session");
        var cleanSaveScan = await _facade.WillInvalidateSignaturesAsync(_session!.SessionId);
        Check(cleanSaveScan is { IsSuccess: true, Value: false }, "Clean signed Save fixture must be incremental-safe");
        var compressAcknowledgement = AskSignatureLossAsync(compressing: true);
        var compressSignatureDialog = await Dialog("Saving will break this document's signature");
        Check(compressSignatureDialog.DefaultButton == ContentDialogButton.Close
            && compressSignatureDialog.PrimaryButtonText == "Save anyway"
            && ((TextBlock)compressSignatureDialog.Content).Text.Contains("invalid rather than missing"),
            "Compression must warn even for clean signed sources, with shared wording/Cancel default");
        Click(compressSignatureDialog, "CloseButton");
        Check(await compressAcknowledgement is null && AnnotationStatus.Text == "Compression cancelled.", "Compression signature cancel status");
        compressAcknowledgement = AskSignatureLossAsync(compressing: true);
        Click(await Dialog("Saving will break this document's signature"), "PrimaryButton");
        Check(await compressAcknowledgement == true, "Compression acknowledgement must be explicit");
        var signedCompression = await _facade.CompressAsync(_session.SessionId, CompressionPreset.Balanced, signaturesAcknowledged: true);
        Check(signedCompression.IsSuccess, "Acknowledged real signed compression: " + signedCompression.Error?.Message);
        Check(!(await _facade.HasUnsavedChangesAsync(_session.SessionId)).Value, "Compression must not dirty the clean signed source");
        var signedExtract = await _facade.PlanExtractPagesAsync(_session.SessionId, new ExtractPagesRequest("1"));
        Check(signedExtract.IsSuccess && signedExtract.Value!.SourceIsSigned, "Extraction must carry signed-source note");
        var signedExtractPath = Path.Combine(Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")!, "signed-extract-" + Guid.NewGuid().ToString("N") + ".pdf");
        await File.WriteAllBytesAsync(signedExtractPath, [1]);
        try
        {
            await WriteExtractedPagesAsync(_session.SessionId, signedExtract.Value!, await StorageFile.GetFileFromPathAsync(signedExtractPath));
            Check(AnnotationStatus.Text.EndsWith("The original document is signed, so the extracted PDF's signature no longer verifies."),
                "Signed extraction must explain the output, not warn about modifying the original: " + AnnotationStatus.Text);
            Check(!(await _facade.HasUnsavedChangesAsync(_session.SessionId)).Value, "Signed extraction keeps source clean");
        }
        finally { File.Delete(signedExtractPath); }
        Check((await _facade.EditPagesAsync(_session!.SessionId, new PageEdit.InsertBlank(0, PageOrientation.Portrait))).IsSuccess,
            "Signed rewrite fixture edit failed");
        var signedSplit = await _facade.PlanSplitPagesAsync(_session.SessionId, "1");
        Check(signedSplit.IsSuccess && signedSplit.Value!.SourceIsSigned, "Split must carry actual signed-source flag");
        var signedSplitPath = Path.Combine(Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")!, "signed-split-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(signedSplitPath);
        try
        {
            await WriteSplitPagesAsync(_session.SessionId, signedSplit.Value!, await StorageFolder.GetFolderFromPathAsync(signedSplitPath));
            Check(AnnotationStatus.Text.EndsWith("The original document is signed, so the signature in each part no longer verifies."),
                "Signed Split completion note: " + AnnotationStatus.Text);
            Check((await _facade.HasUnsavedChangesAsync(_session.SessionId)).Value, "Split must preserve pending signed-source edits");
        }
        finally { Directory.Delete(signedSplitPath, recursive: true); }
        var signatureQuery = await _facade.WillInvalidateSignaturesAsync(_session.SessionId);
        Check(signatureQuery is { IsSuccess: true, Value: true }, "Signed edited source must require warning: " + signatureQuery.Error?.Message);
        var acknowledgement = AskSignatureLossAsync();
        var signatureDialog = await Dialog("Saving will break this document's signature");
        Check(signatureDialog.DefaultButton == ContentDialogButton.Close
            && ((TextBlock)signatureDialog.Content).Text.Contains("invalid rather than missing"), "Signature warning must default to cancellation and describe invalidation");
        Click(signatureDialog, "CloseButton");
        Check(await acknowledgement is null, "Cancelling must not acknowledge signature loss");
        acknowledgement = AskSignatureLossAsync();
        Click(await Dialog("Saving will break this document's signature"), "PrimaryButton");
        Check(await acknowledgement == true, "Save anyway must explicitly acknowledge signature loss");
        var unsigned = await _facade.OpenAsync(new DocumentSource(SampleDisplayName, await File.ReadAllBytesAsync(SamplePath)), discardPendingEdits: true);
        Check(unsigned.IsSuccess, "Restore unsigned fixture");
        ShowOpenedDocument(unsigned.Value!);
        var scratch = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")!;
        var destination = Path.Combine(scratch, "lifecycle-save-" + Guid.NewGuid().ToString("N") + ".pdf");
        await File.WriteAllBytesAsync(destination, [1, 2, 3]);
        try
        {
            var file = await StorageFile.GetFileFromPathAsync(destination);
            Check((await _facade.SetDocumentPropertyAsync(_session!.SessionId, DocumentProperty.Title, "Save smoke")).IsSuccess, "Save fixture edit failed");
            using (var locked = new FileStream(destination, FileMode.Open, FileAccess.ReadWrite, FileShare.None))
            {
                Check(!await WritePickedPdfAsync(_session.SessionId, file, false), "Locked destination must refuse replacement");
                Check((await _facade.HasUnsavedChangesAsync(_session.SessionId)).Value && AnnotationStatus.Text.Length > 0,
                    "Failed atomic write must keep pending work and surface the error");
            }
            Check((await File.ReadAllBytesAsync(destination)).SequenceEqual(new byte[] { 1, 2, 3 }), "Failed replacement must not change the destination");
            Check(await WritePickedPdfAsync(_session.SessionId, file, false), "Atomic write must succeed after lock release");
            Check(!(await _facade.HasUnsavedChangesAsync(_session.SessionId)).Value && new FileInfo(destination).Length > 100,
                "Successful write must persist bytes and clear unsaved state");
        }
        finally { File.Delete(destination); }
        original = _session!.SessionId;
        var edit = await _facade.SetDocumentPropertyAsync(original, DocumentProperty.Title, "Lifecycle smoke");
        Check(edit.IsSuccess, "Metadata fixture edit failed");
        var creating = CreateNewDocumentAsync();
        Click(await Dialog("Unsaved changes"), "CloseButton");
        await creating;
        Check(_session!.SessionId == original && (await _facade.HasUnsavedChangesAsync(original)).Value, "New-document Cancel must preserve work");
        creating = CreateNewDocumentAsync();
        Click(await Dialog("Unsaved changes"), "SecondaryButton");
        await creating;
        Check(_session!.SessionId != original && _session.PageCount == 1
            && Math.Abs(_session.Pages[0].WidthPt - 595.28) < 1 && Math.Abs(_session.Pages[0].HeightPt - 841.89) < 1,
            "Discard must open a fresh portrait A4 page");
        Check(!(await _facade.HasUnsavedChangesAsync(_session.SessionId)).Value, "Fresh blank page is not an edit");
        Check((await _facade.SetDocumentPropertyAsync(_session.SessionId, DocumentProperty.Title, "Close smoke")).IsSuccess, "Close fixture edit failed");
        var handle = WinRT.Interop.WindowNative.GetWindowHandle(this);
        Check(PostMessage(handle, 0x0010, IntPtr.Zero, IntPtr.Zero), "Native close request failed");
        var closeDialog = await Dialog("Unsaved changes");
        Check(((TextBlock)closeDialog.Content).Text.EndsWith("Closing Vitela will discard them."), "Close must have its own explanation");
        Click(closeDialog, "CloseButton");
        await Wait(() => !_confirmingClose, "Close Cancel did not settle");
        Check(!_windowClosed && (await _facade.HasUnsavedChangesAsync(_session.SessionId)).Value, "Close Cancel must preserve the live window/work");
        Check(PostMessage(handle, 0x0010, IntPtr.Zero, IntPtr.Zero), "Second native close request failed");
        Click(await Dialog("Unsaved changes"), "SecondaryButton");
        await Wait(() => _approvedClose && _windowClosed, "Approved programmatic close did not destroy the window");
    }
}
