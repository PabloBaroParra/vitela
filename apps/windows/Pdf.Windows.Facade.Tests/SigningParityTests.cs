using Pdf.Windows.Facade;

internal static class SigningParityTests
{
    private static void Check(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }

    public static async Task RunAsync()
    {
        var core = new FakeCore { ExtractedSourceSigned = true };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new("sign.pdf", [1]))).Value!;
        var state = await facade.SigningStateAsync(session.SessionId);
        Check(state.IsSuccess && state.Value!.IsSigned && state.Value.Refusal is null, "Existing signature indicator must be independent of eligibility");
        Check(!(await facade.SigningStateAsync("stale")).IsSuccess, "Stale state query must fail");
        using var certificate = (await facade.OpenSigningCertificateAsync([1], "")).Value!;
        var wrote = false;
        core.SignRefusal = "this document does not permit adding a signature";
        var refused = await facade.SignToDestinationAsync(session.SessionId, certificate, "test", "signed.pdf", _ => { wrote = true; return Task.CompletedTask; });
        Check(!refused.IsSuccess && !wrote, "Core refusal must precede writing");
        core.SignRefusal = null;
        var failed = await facade.SignToDestinationAsync(session.SessionId, certificate, "test", "signed.pdf", _ => throw new IOException("write failed"));
        Check(!failed.IsSuccess && (await facade.SessionAsync(session.SessionId)).IsSuccess, "Failed write must retain original session");
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var release = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var sign = facade.SignToDestinationAsync(session.SessionId, certificate, "test", "signed.pdf", async bytes =>
        {
            Check(bytes.SequenceEqual(new byte[] { 1, 2, 3 }), "Signed bytes must reach destination unchanged");
            entered.SetResult();
            await release.Task;
        });
        await entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
        var open = facade.OpenAsync(new("next.pdf", [1]));
        Check(!open.IsCompleted, "Concurrent open must wait for sign/write/reopen");
        release.SetResult();
        var signed = await sign;
        Check(signed.IsSuccess && signed.Value!.DisplayName == "signed.pdf" && signed.Value.SessionId != session.SessionId, "Successful signing must reopen as a new session");
        Check((await open).IsSuccess, "Queued open must resume");
        Check(!(await facade.SignToDestinationAsync(session.SessionId, certificate, "test", "stale.pdf", _ => { wrote = true; return Task.CompletedTask; })).IsSuccess && !wrote, "Stale sign must not write");

        using var empty = new PdfDocumentFacade(new FakeCore { PageCount = 0 }, new RecordingLogger());
        var blank = (await empty.OpenAsync(new("empty.pdf", [1]))).Value!;
        Check((await empty.SigningStateAsync(blank.SessionId)).Value!.Refusal is not null, "Zero-page state must refuse signing");

        var protectedCore = new FakeCore { RequiredPassword = "test-password" };
        using var protectedFacade = new PdfDocumentFacade(protectedCore, new RecordingLogger());
        var protectedSession = (await protectedFacade.OpenAsync(new("protected.pdf", [1]), "test-password")).Value!;
        var protectedDocument = protectedCore.LastDocument;
        var protectedSigned = await protectedFacade.SignToDestinationAsync(protectedSession.SessionId, certificate, "test", "protected-signed.pdf", _ => Task.CompletedTask);
        Check(protectedSigned.IsSuccess && ReferenceEquals(protectedCore.SignedReopenSource, protectedDocument), "Signing must delegate credential-preserving reopen to the source handle, not open without a password");
    }
}

internal sealed class FakeSigningCertificate : ISigningCertificate
{
    public IReadOnlyList<SigningIdentity> Identities => [new("test", "Test identity")];
    public void Dispose() { }
}
