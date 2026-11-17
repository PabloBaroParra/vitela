namespace Pdf.Windows.Recovery;

/// <summary>
/// When autosave tries again after a write failed.
/// </summary>
/// <remarks>
/// A full disk or an unwritable profile fails every snapshot the same way, and
/// each attempt costs a full serialization of the document first. So a failure
/// pushes the next attempt out — one interval, then twice that, doubling up to
/// <see cref="MaxDelay"/> — and the first success resets the schedule. The
/// caller supplies the clock (any monotonic <see cref="TimeSpan"/>), so the
/// rule is tested without one.
/// </remarks>
public sealed class AutosaveBackoff
{
    /// <summary>The wait after the first failure: one ordinary autosave interval.</summary>
    public static readonly TimeSpan FirstDelay = AutosaveScheduler.Interval;

    /// <summary>The longest a failing autosave waits between attempts.</summary>
    public static readonly TimeSpan MaxDelay = TimeSpan.FromMinutes(15);

    private int _failures;
    private TimeSpan? _retryAt;

    /// <summary>How long to wait after the <paramref name="failures"/>-th failure in a row.</summary>
    public static TimeSpan Delay(int failures)
    {
        var doublings = Math.Min(Math.Max(failures - 1, 0), 16);
        var delay = FirstDelay * (1 << doublings);
        return delay < MaxDelay ? delay : MaxDelay;
    }

    /// <summary>Whether an attempt may be made at <paramref name="now"/>.</summary>
    public bool Ready(TimeSpan now) => _retryAt is not { } retryAt || now >= retryAt;

    /// <summary>Records a failed attempt. True for the first failure of a streak — the one the user is told about.</summary>
    public bool Failed(TimeSpan now)
    {
        _failures = _failures == int.MaxValue ? _failures : _failures + 1;
        _retryAt = now + Delay(_failures);
        return _failures == 1;
    }

    /// <summary>A write landed: the next failure starts a new streak.</summary>
    public void Succeeded()
    {
        _failures = 0;
        _retryAt = null;
    }
}
