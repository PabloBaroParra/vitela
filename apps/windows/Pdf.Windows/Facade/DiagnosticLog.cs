namespace Pdf.Windows.Facade;

/// <summary>
/// Writes failure diagnostics somewhere they can be read back.
///
/// Every failure the shell shows is deliberately vague — "The document could
/// not be processed." — and carries a correlation id so the real cause can be
/// looked up. That lookup had nowhere to happen: the detail went only to
/// <c>Debug.WriteLine</c>, which reaches an attached debugger and nothing else,
/// and is compiled out of a Release build entirely. The id shown to the user
/// was a reference to a record that did not exist.
///
/// Entries are the same sanitized fields the interface already carries — no
/// paths, no document content, no passwords. A session id is a GUID minted per
/// open, so it correlates without identifying anything.
/// </summary>
internal sealed class FileDiagnosticLogger : IDiagnosticLogger
{
    /// <summary>
    /// Kept small on purpose: this is a tail for the last failures, not an
    /// audit trail, and it must not grow without bound on a machine nobody is
    /// watching.
    /// </summary>
    internal const long DefaultMaxBytes = 256 * 1024;

    private readonly string _path;
    private readonly long _maxBytes;
    private readonly object _gate = new();

    internal FileDiagnosticLogger(string path, long maxBytes = DefaultMaxBytes)
    {
        _path = path;
        _maxBytes = maxBytes;
    }

    /// <summary>
    /// <c>%LOCALAPPDATA%\Vitela\diagnostics.log</c> — per-user, roaming-free,
    /// and writable by an unpackaged app without elevation.
    /// </summary>
    internal static string DefaultPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Vitela",
        "diagnostics.log");

    public void Failure(PdfCoreError category, string operation, string correlationId, string? sessionId, uint? pageIndex, string sanitizedDetail)
    {
        var line = $"{DateTime.UtcNow:yyyy-MM-ddTHH:mm:ss.fffZ} {category} {operation} {correlationId} session={sessionId ?? "-"} page={pageIndex?.ToString() ?? "-"} {sanitizedDetail}";
        System.Diagnostics.Debug.WriteLine($"PDF failure {line}");

        try
        {
            lock (_gate)
            {
                if (Path.GetDirectoryName(_path) is { Length: > 0 } directory)
                {
                    Directory.CreateDirectory(directory);
                }
                RotateIfOversizeLocked();
                File.AppendAllText(_path, line + Environment.NewLine);
            }
        }
        catch (Exception)
        {
            // Diagnostics must never become the failure they exist to explain.
            // A full disk or a locked file loses the entry; the operation that
            // produced it still reports to the user exactly as before.
        }
    }

    /// <summary>
    /// One generation back is kept, so a failure is still readable after a
    /// noisy session pushed the log over its cap.
    /// </summary>
    private void RotateIfOversizeLocked()
    {
        var current = new FileInfo(_path);
        if (!current.Exists || current.Length < _maxBytes) return;

        var previous = _path + ".1";
        File.Delete(previous);
        File.Move(_path, previous);
    }
}
