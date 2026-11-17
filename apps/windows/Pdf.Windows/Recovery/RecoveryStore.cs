using System.Runtime.Versioning;
using System.Security.AccessControl;
using System.Security.Principal;

namespace Pdf.Windows.Recovery;

/// <summary>
/// The recovery directory on disk — no window, no core, only files.
/// </summary>
/// <remarks>
/// <para>
/// One entry per window that has unsaved work, named by an <see cref="EntryKey"/>
/// the window mints once. An entry is three files side by side:
/// <c>&lt;key&gt;.pdf</c> (the document's bytes, pending edits applied),
/// <c>&lt;key&gt;.meta</c> (a <see cref="RecoveryMeta"/> sidecar) and
/// <c>&lt;key&gt;.lock</c>, held open with <see cref="FileShare.None"/> by the
/// process that owns the entry for as long as it lives.
/// </para>
/// <para>
/// The lock is what tells a crash from a running window. Windows closes a dead
/// process's handles however it died, so on the next launch an entry whose lock
/// can be opened is one nobody is writing any more: an orphan, offered for
/// recovery. One whose lock is held belongs to a window still open — another
/// instance — and is left alone. A PID could not tell the two apart once it is
/// reused.
/// </para>
/// <para>
/// The directory is under <c>%LOCALAPPDATA%</c> — per user, never roaming — and
/// its access list is narrowed to the current user alone, inherited by every
/// file in it. Both data files are written to a sibling and moved into place,
/// the sidecar last, so an entry is only ever complete or absent.
/// </para>
/// </remarks>
public sealed class RecoveryDirectory(string root)
{
    internal const string PdfExtension = "pdf";
    internal const string MetaExtension = "meta";
    internal const string LockExtension = "lock";

    /// <summary><c>%LOCALAPPDATA%\Vitela\recovery</c>, beside the diagnostics log and the signature store.</summary>
    public static string DefaultRoot => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Vitela",
        "recovery");

    public string Root => root;

    /// <summary>Opens and locks the entry <paramref name="key"/> names, creating the directory first.</summary>
    /// <exception cref="IOException">The directory or the lock could not be made.</exception>
    /// <exception cref="UnauthorizedAccessException">The directory is not writable.</exception>
    public RecoverySlot OpenSlot(EntryKey key)
    {
        Ensure();
        var paths = PathsOf(key.Value);
        // A fresh key is never held by anyone else; failing here means no
        // entry could ever be told apart from a live one — better to write none.
        return new RecoverySlot(paths, OpenLock(paths.LockPath));
    }

    /// <summary>
    /// Every entry no live process holds, newest first, each claimed so no other
    /// instance can offer or delete it meanwhile.
    /// </summary>
    /// <remarks>
    /// Entries that cannot be recovered — a sidecar that does not parse, bytes
    /// that are missing, a write a crash cut short — are deleted here with a log
    /// line rather than offered or left to pile up.
    /// </remarks>
    public IReadOnlyList<RecoveryOrphan> Orphans(Action<string> log)
    {
        string[] names;
        try
        {
            if (!Directory.Exists(root)) return [];
            names = [.. Directory.EnumerateFiles(root).Select(Path.GetFileName).OfType<string>()];
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            log($"recovery: cannot list {root}: {error.Message}");
            return [];
        }
        return [.. names
            .Select(KeyOf)
            .OfType<string>()
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .Select(key => Claim(key, log))
            .OfType<RecoveryOrphan>()
            .OrderByDescending(orphan => orphan.Meta.SavedAt)];
    }

    private RecoveryOrphan? Claim(string key, Action<string> log)
    {
        var paths = PathsOf(key);
        FileStream lockFile;
        try
        {
            lockFile = OpenLock(paths.LockPath);
        }
        catch (IOException error) when (IsHeld(error))
        {
            // A window that is still open owns this entry.
            return null;
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            log($"recovery: cannot lock {paths.LockPath}: {error.Message}");
            return null;
        }

        RemoveTemporaries(paths);
        RecoveryMeta? meta = null;
        try
        {
            if (File.Exists(paths.MetaPath)) meta = RecoveryMeta.Parse(File.ReadAllText(paths.MetaPath));
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException) { }
        if (meta is not null && File.Exists(paths.PdfPath)) return new RecoveryOrphan(paths, meta, lockFile);

        log($"recovery: discarding incomplete or unreadable entry {Path.Combine(root, key)}");
        paths.RemoveAll(lockFile);
        return null;
    }

    internal EntryPaths PathsOf(string key)
    {
        string File(string extension) => Path.Combine(root, $"{key}.{extension}");
        return new EntryPaths(File(PdfExtension), File(MetaExtension), File(LockExtension));
    }

    private static void RemoveTemporaries(EntryPaths paths)
    {
        foreach (var path in new[] { paths.PdfPath, paths.MetaPath })
        {
            try { File.Delete(EntryPaths.TemporaryOf(path)); }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException) { }
        }
    }

    /// <summary>Creates the directory and narrows it to the current user: it is about to hold their documents.</summary>
    private void Ensure()
    {
        var created = !Directory.Exists(root);
        Directory.CreateDirectory(root);
        if (OperatingSystem.IsWindows()) RestrictToCurrentUser(root, created);
    }

    /// <summary>
    /// Replaces the inherited access list with one rule: the current user, full
    /// control, inherited by everything inside. Done on every open, so a
    /// directory loosened since is tightened again. Best effort on top of the
    /// per-user profile: a file system without access lists keeps the profile's.
    /// </summary>
    [SupportedOSPlatform("windows")]
    private static void RestrictToCurrentUser(string directory, bool created)
    {
        try
        {
            if (WindowsIdentity.GetCurrent().User is not { } user) return;
            var security = new DirectorySecurity();
            security.SetAccessRuleProtection(isProtected: true, preserveInheritance: false);
            security.AddAccessRule(new FileSystemAccessRule(
                user,
                FileSystemRights.FullControl,
                InheritanceFlags.ContainerInherit | InheritanceFlags.ObjectInherit,
                PropagationFlags.None,
                AccessControlType.Allow));
            new DirectoryInfo(directory).SetAccessControl(security);
        }
        catch (Exception error) when (error is UnauthorizedAccessException or IOException or PlatformNotSupportedException or InvalidOperationException)
        {
            System.Diagnostics.Trace.WriteLine($"recovery: cannot restrict {directory} to the current user ({(created ? "new" : "existing")}): {error.Message}");
        }
    }

    private static FileStream OpenLock(string path) =>
        new(path, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None);

    /// <summary>Sharing or lock violation: another handle holds the file.</summary>
    private static bool IsHeld(IOException error) => (error.HResult & 0xFFFF) is 32 or 33;

    /// <summary>
    /// The entry key a file in the directory belongs to: the stem of
    /// <c>&lt;key&gt;.pdf</c>, <c>.meta</c>, <c>.lock</c>, or of a temporary
    /// <c>.&lt;key&gt;.pdf.tmp</c>. <c>null</c> for anything else, which the
    /// scan leaves alone.
    /// </summary>
    internal static string? KeyOf(string name)
    {
        if (name.StartsWith('.'))
        {
            if (!name.EndsWith(".tmp", StringComparison.Ordinal)) return null;
            name = name[1..^4];
        }
        var dot = name.LastIndexOf('.');
        if (dot <= 0) return null;
        var (key, extension) = (name[..dot], name[(dot + 1)..]);
        var ours = extension is PdfExtension or MetaExtension or LockExtension;
        var shaped = key.All(character => char.IsAsciiDigit(character) || character == '-');
        return ours && shaped ? key : null;
    }
}

/// <summary>
/// Names one window's entry: process id, the moment the key was minted, and a
/// per-process sequence number, so neither two windows of one process nor a
/// later process that reuses the PID lands on the same files.
/// </summary>
public sealed record EntryKey(string Value)
{
    private static long s_sequence;

    public static EntryKey Fresh() =>
        new($"{Environment.ProcessId}-{DateTime.UtcNow.Ticks}-{Interlocked.Increment(ref s_sequence)}");
}

/// <summary>The paths of one entry. Plain strings, so a worker thread can write them.</summary>
public sealed record EntryPaths(string PdfPath, string MetaPath, string LockPath)
{
    /// <summary>Replaces the entry's bytes and sidecar, each atomically, sidecar last.</summary>
    public void Write(byte[] bytes, RecoveryMeta meta)
    {
        WriteAtomically(PdfPath, bytes);
        WriteAtomically(MetaPath, System.Text.Encoding.UTF8.GetBytes(meta.Encode()));
    }

    /// <summary>Deletes the bytes and the sidecar, keeping the lock — for a write that finished after its entry was forgotten.</summary>
    public void RemoveData()
    {
        // Sidecar first: without it the entry is never offered, even if
        // removing the bytes then fails.
        TryDelete(MetaPath);
        TryDelete(PdfPath);
    }

    internal void RemoveAll(FileStream lockFile)
    {
        RemoveData();
        lockFile.Dispose();
        TryDelete(LockPath);
    }

    internal static string TemporaryOf(string path) =>
        Path.Combine(Path.GetDirectoryName(path)!, $".{Path.GetFileName(path)}.tmp");

    private static void WriteAtomically(string path, byte[] bytes)
    {
        var temporary = TemporaryOf(path);
        TryDelete(temporary);
        try
        {
            using (var file = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None))
            {
                file.Write(bytes);
                file.Flush(flushToDisk: true);
            }
            File.Move(temporary, path, overwrite: true);
        }
        catch
        {
            TryDelete(temporary);
            throw;
        }
    }

    private static void TryDelete(string path)
    {
        try { File.Delete(path); }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException) { }
    }
}

/// <summary>
/// An entry this process owns and writes, locked until <see cref="Clear"/> —
/// or until the process dies, which leaves the files for the next launch.
/// </summary>
public sealed class RecoverySlot(EntryPaths paths, FileStream lockFile)
{
    public EntryPaths Paths => paths;

    /// <summary>Deletes the entry: the document was saved, closed or discarded.</summary>
    public void Clear() => paths.RemoveAll(lockFile);

    /// <summary>
    /// Lets go the way a crash does: the lock is released and the files stay.
    /// For tests and the recovery smoke; a running window never calls it.
    /// </summary>
    internal void ReleaseLikeACrash() => lockFile.Dispose();
}

/// <summary>
/// An entry left by a process that is gone, claimed by this one. Disposing it
/// releases the claim and keeps the files, to be offered again next launch.
/// </summary>
public sealed class RecoveryOrphan(EntryPaths paths, RecoveryMeta meta, FileStream lockFile) : IDisposable
{
    public RecoveryMeta Meta => meta;

    /// <summary>The recovered bytes.</summary>
    /// <exception cref="InvalidDataException">They do not even start like a PDF.</exception>
    public byte[] ReadPdf()
    {
        var bytes = File.ReadAllBytes(paths.PdfPath);
        if (!bytes.AsSpan().StartsWith("%PDF-"u8)) throw new InvalidDataException("the recovered file is not a PDF");
        return bytes;
    }

    /// <summary>
    /// Whether a <see cref="ReadPdf"/> failure says the bytes can never be
    /// restored. Only their own verdict does; the disk failing to hand them
    /// over says nothing about them.
    /// </summary>
    public static bool IsUnrecoverableReadFailure(Exception error) => error is InvalidDataException;

    /// <summary>Deletes the entry: restored and safely re-recorded, or discarded.</summary>
    public void Discard() => paths.RemoveAll(lockFile);

    public void Dispose() => lockFile.Dispose();
}
