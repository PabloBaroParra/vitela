namespace Pdf.Windows.Facade;

/// <summary>
/// The one drawn signature the reader asked to be remembered on this PC, as the
/// PNG the pad made. Every method touches storage, so callers run them off the
/// UI thread.
/// </summary>
public interface ISignatureStore
{
    /// <summary>The remembered PNG, or null when there is none or it cannot be read.</summary>
    byte[]? Load();

    /// <summary>Replaces the remembered PNG; false when it could not be written (the old one is kept).</summary>
    bool Save(byte[] png);

    /// <summary>Forgets the remembered signature. Nothing to forget is not an error.</summary>
    void Delete();
}

/// <summary>Remembers nothing — for a shell built without app storage.</summary>
public sealed class NoSignatureStore : ISignatureStore
{
    public static readonly NoSignatureStore Instance = new();

    public byte[]? Load() => null;
    public bool Save(byte[] png) => false;
    public void Delete() { }
}

/// <summary>
/// The signature kept in one file under <c>%LOCALAPPDATA%\Vitela</c> — the
/// folder the diagnostics log already uses. Local, not roaming: a signature
/// must not follow the reader's account to other PCs. Written to a sibling and
/// moved over, so a crash mid-write leaves the old signature, not half of the
/// new one.
/// </summary>
public sealed class FileSignatureStore(string path) : ISignatureStore
{
    public static string DefaultPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Vitela",
        "signature.png");

    private string Partial => path + ".partial";

    public byte[]? Load()
    {
        try
        {
            return File.Exists(path) && File.ReadAllBytes(path) is { Length: > 0 } bytes ? bytes : null;
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            return null;
        }
    }

    public bool Save(byte[] png)
    {
        try
        {
            if (Path.GetDirectoryName(path) is { Length: > 0 } directory) Directory.CreateDirectory(directory);
            File.WriteAllBytes(Partial, png);
            File.Move(Partial, path, overwrite: true);
            return true;
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            try { File.Delete(Partial); } catch (Exception) { /* a leftover sibling is overwritten by the next save */ }
            return false;
        }
    }

    public void Delete()
    {
        try { File.Delete(path); } catch (Exception error) when (error is IOException or UnauthorizedAccessException) { }
    }
}
