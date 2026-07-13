// UniFFI C# spike host (T-006..T-010, T-070): exercises the uniffi-bindgen-cs
// generated bindings for uniffi_cs_spike from a real, running C# process.
// No simulated numbers — every timing printed here comes from an actual
// Stopwatch measurement around a real native call.

using System.Diagnostics;
using System.Globalization;
using uniffi.uniffi_cs_spike;

CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;

int failures = 0;

void Check(string label, bool condition)
{
    if (condition)
    {
        Console.WriteLine($"[PASS] {label}");
    }
    else
    {
        Console.WriteLine($"[FAIL] {label}");
        failures++;
    }
}

Console.WriteLine("=== T-006: string echo + byte-array round-trip ===");

var echoed = UniffiCsSpikeMethods.EchoString("héllo wörld 日本語");
Check("echo_string round-trips non-ASCII", echoed == "héllo wörld 日本語");

var smallInput = new byte[] { 0, 1, 2, 3, 255, 254 };
var smallOutput = UniffiCsSpikeMethods.BytesRoundTrip(smallInput);
Check("bytes_round_trip small buffer identical", smallInput.SequenceEqual(smallOutput));

Console.WriteLine();
Console.WriteLine("=== T-007: >=8MB buffer round-trip benchmark ===");

const int bufferSize = 8 * 1024 * 1024 + 37; // >=8MB, matches BitmapHandle.get_pixels() payload size
var largeBuffer = new byte[bufferSize];
new Random(42).NextBytes(largeBuffer);

const int warmupIterations = 3;
const int measuredIterations = 20;

for (int i = 0; i < warmupIterations; i++)
{
    _ = UniffiCsSpikeMethods.BytesRoundTrip(largeBuffer);
}

var timingsMs = new List<double>();
byte[]? lastOutput = null;
for (int i = 0; i < measuredIterations; i++)
{
    var sw = Stopwatch.StartNew();
    lastOutput = UniffiCsSpikeMethods.BytesRoundTrip(largeBuffer);
    sw.Stop();
    timingsMs.Add(sw.Elapsed.TotalMilliseconds);
}

Check("large buffer round-trip identical", lastOutput != null && largeBuffer.SequenceEqual(lastOutput));

timingsMs.Sort();
double min = timingsMs[0];
double max = timingsMs[^1];
double avg = timingsMs.Average();
double p50 = timingsMs[timingsMs.Count / 2];

Console.WriteLine($"buffer_size_bytes={bufferSize}");
Console.WriteLine($"iterations={measuredIterations} (+{warmupIterations} warmup, discarded)");
Console.WriteLine($"min_ms={min:F3}");
Console.WriteLine($"p50_ms={p50:F3}");
Console.WriteLine($"avg_ms={avg:F3}");
Console.WriteLine($"max_ms={max:F3}");

Console.WriteLine();
Console.WriteLine("=== T-008: error-enum -> C# exception mapping ===");

try
{
    var ok = UniffiCsSpikeMethods.CheckedDivide(10, 2);
    Check("checked_divide happy path", ok == 5);
}
catch (Exception ex)
{
    Check($"checked_divide happy path (unexpected exception {ex.GetType().Name})", false);
}

try
{
    UniffiCsSpikeMethods.CheckedDivide(7, 0);
    Check("checked_divide by zero should have thrown", false);
}
catch (SpikeException.DivideByZero)
{
    Check("checked_divide by zero throws typed SpikeException.DivideByZero (not a string)", true);
}
catch (Exception ex)
{
    Check($"checked_divide by zero threw wrong type: {ex.GetType().FullName}", false);
}

try
{
    UniffiCsSpikeMethods.RequireNonEmpty(Array.Empty<byte>());
    Check("require_non_empty on empty should have thrown", false);
}
catch (SpikeException.EmptyInput)
{
    Check("require_non_empty on empty throws typed SpikeException.EmptyInput (not a string)", true);
}
catch (Exception ex)
{
    Check($"require_non_empty on empty threw wrong type: {ex.GetType().FullName}", false);
}

Console.WriteLine();
Console.WriteLine("=== T-009: callback/event delivery, Rust -> C# ===");

var received = new List<(uint Sequence, string Message)>();
var allReceived = new ManualResetEventSlim(false);
const uint expectedCount = 5;

var listener = new RecordingListener(received, allReceived, expectedCount);

var callStart = Stopwatch.StartNew();
UniffiCsSpikeMethods.FireEvents(listener, expectedCount);
var callReturnedMs = callStart.Elapsed.TotalMilliseconds;

// fire_events is fire-and-forget on the Rust side (spawns a background OS
// thread); the call must return near-instantly, well before all events
// have been delivered (each event is spaced 20ms apart Rust-side, so 5
// events take >=100ms to fully deliver).
Check($"fire_events call returns immediately (async contract, took {callReturnedMs:F2}ms)", callReturnedMs < 50);

bool deliveredInTime = allReceived.Wait(TimeSpan.FromSeconds(5));
Check("all callback events delivered within timeout", deliveredInTime);
Check($"received exactly {expectedCount} events", received.Count == (int)expectedCount);

bool inOrder = true;
for (int i = 0; i < received.Count; i++)
{
    if (received[i].Sequence != (uint)i || received[i].Message != $"event-{i}")
    {
        inOrder = false;
        break;
    }
}
Check("events delivered in order with correct payload", inOrder);

Console.WriteLine();
Console.WriteLine("=== T-070: synchronous callback with a return value ===");

var digest = new byte[] { 0x00, 0x12, 0xA5, 0xFF };
var signature = UniffiCsSpikeMethods.RequestDigestSignature(
    new DeterministicDigestSigner(),
    digest);
Check(
    "sign_digest synchronously returns signature bytes",
    signature.SequenceEqual(new byte[] { 0xA5, 0xB7, 0x00, 0x5A }));

try
{
    UniffiCsSpikeMethods.RequestDigestSignature(
        new UnavailableDigestSigner(),
        digest);
    Check("sign_digest should have returned a typed callback error", false);
}
catch (SigningCallbackException.IdentityUnavailable)
{
    Check("sign_digest propagates typed callback error", true);
}
catch (Exception ex)
{
    Check($"sign_digest propagated wrong error type: {ex.GetType().FullName}", false);
}

Console.WriteLine();
Console.WriteLine($"=== RESULT: {(failures == 0 ? "ALL CHECKS PASSED" : $"{failures} CHECK(S) FAILED")} ===");
return failures == 0 ? 0 : 1;

class RecordingListener : SpikeEventListener
{
    private readonly List<(uint Sequence, string Message)> _received;
    private readonly ManualResetEventSlim _done;
    private readonly uint _expectedCount;
    private readonly object _lock = new();

    public RecordingListener(List<(uint, string)> received, ManualResetEventSlim done, uint expectedCount)
    {
        _received = received;
        _done = done;
        _expectedCount = expectedCount;
    }

    public void OnEvent(uint sequence, string message)
    {
        lock (_lock)
        {
            _received.Add((sequence, message));
            if (_received.Count >= _expectedCount)
            {
                _done.Set();
            }
        }
    }
}

class DeterministicDigestSigner : DigestSigner
{
    public byte[] SignDigest(byte[] digest)
    {
        return digest.Select(value => (byte)(value ^ 0xA5)).ToArray();
    }
}

class UnavailableDigestSigner : DigestSigner
{
    public byte[] SignDigest(byte[] digest)
    {
        throw new SigningCallbackException.IdentityUnavailable();
    }
}
