//! The one line a measurement becomes — see `docs/perf-baseline.md` for the
//! format this module is the single writer of.

use std::fmt::Write as _;
use std::io;
use std::time::Duration;

/// A field's value. Only what is free to hand over: numbers, flags and
/// compile-time names — never an owned string, so building a field list
/// allocates nothing whether or not the traces are on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Value {
    Int(u64),
    Float(f64),
    Text(&'static str),
    Flag(bool),
}

/// One `key=value` pair of a line.
pub(crate) type Field = (&'static str, Value);

impl From<u64> for Value {
    fn from(value: u64) -> Self {
        Value::Int(value)
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Value::Int(u64::from(value))
    }
}

impl From<usize> for Value {
    fn from(value: usize) -> Self {
        Value::Int(value as u64)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Float(value)
    }
}

impl From<&'static str> for Value {
    fn from(value: &'static str) -> Self {
        Value::Text(value)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Flag(value)
    }
}

/// `perf <name> <millis>ms key=value ...`, without a line ending.
///
/// Milliseconds carry three decimals (microsecond resolution); fields keep
/// the order they were given in.
pub(super) fn format_line(name: &str, elapsed: Duration, fields: &[Field]) -> String {
    let mut line = format!("perf {name} {:.3}ms", elapsed.as_secs_f64() * 1000.0);
    for (key, value) in fields {
        // Writing into a `String` cannot fail.
        let _ = match value {
            Value::Int(value) => write!(line, " {key}={value}"),
            Value::Float(value) => write!(line, " {key}={value:.3}"),
            Value::Text(value) => write!(line, " {key}={value}"),
            Value::Flag(value) => write!(line, " {key}={value}"),
        };
    }
    line
}

/// The line for a measurement of `elapsed`, or `None` when it is under
/// `floor` — how a span that fires on every scroll tick stays quiet.
pub(super) fn measurement(
    name: &str,
    elapsed: Duration,
    floor: Duration,
    fields: &[Field],
) -> Option<String> {
    (elapsed >= floor).then(|| format_line(name, elapsed, fields))
}

/// Writes `line` and its line ending to `sink` in one call, so lines from
/// different threads never interleave mid-line.
pub(super) fn write_line(sink: &mut impl io::Write, line: &str) -> io::Result<()> {
    let mut terminated = String::with_capacity(line.len() + 1);
    terminated.push_str(line);
    terminated.push('\n');
    sink.write_all(terminated.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_without_fields_is_the_name_and_the_milliseconds() {
        assert_eq!(
            format_line("open.worker", Duration::from_micros(12_345), &[]),
            "perf open.worker 12.345ms"
        );
    }

    #[test]
    fn fields_follow_in_the_order_given() {
        let fields = [
            ("pages", Value::from(1600_usize)),
            ("zoom", Value::from(1.5_f64)),
            ("view", Value::from("pages")),
            ("ok", Value::from(true)),
        ];
        assert_eq!(
            format_line("organize.show", Duration::from_millis(7), &fields),
            "perf organize.show 7.000ms pages=1600 zoom=1.500 view=pages ok=true"
        );
    }

    #[test]
    fn a_sub_millisecond_measurement_keeps_its_microseconds() {
        assert_eq!(
            format_line("viewport.update", Duration::from_micros(42), &[]),
            "perf viewport.update 0.042ms"
        );
    }

    #[test]
    fn a_measurement_under_its_floor_is_not_reported() {
        let floor = Duration::from_millis(2);
        assert_eq!(
            measurement("viewport.update", Duration::from_micros(1_999), floor, &[]),
            None
        );
    }

    #[test]
    fn a_measurement_at_or_over_its_floor_is_reported() {
        let floor = Duration::from_millis(2);
        assert_eq!(
            measurement("viewport.update", floor, floor, &[]).as_deref(),
            Some("perf viewport.update 2.000ms")
        );
        assert!(measurement("layout.refresh", Duration::ZERO, Duration::ZERO, &[]).is_some());
    }

    #[test]
    fn a_line_reaches_the_sink_whole_and_terminated() {
        let mut sink = Vec::new();
        write_line(&mut sink, "perf save.worker 3.000ms").expect("a Vec never fails");
        write_line(&mut sink, "perf save.rebuild 1.000ms").expect("a Vec never fails");
        assert_eq!(
            String::from_utf8(sink).expect("lines are UTF-8"),
            "perf save.worker 3.000ms\nperf save.rebuild 1.000ms\n"
        );
    }
}
