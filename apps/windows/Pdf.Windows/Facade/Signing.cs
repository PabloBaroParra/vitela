namespace Pdf.Windows.Facade;

public sealed record SigningIdentity(string Id, string DisplayName);
public sealed record SigningState(string SessionId, string? Refusal, bool IsSigned);

/// <summary>Owns an unlocked private key; dispose as soon as the signing flow ends.</summary>
public interface ISigningCertificate : IDisposable
{
    IReadOnlyList<SigningIdentity> Identities { get; }
}
