//! Document Info Dictionary (`/Info`) model (T-167, Batch 22 — see
//! `docs/batch-metadata-edit.md`).
//!
//! Scope is the seven standard text keys plus the two date keys — `/Trapped`
//! and custom keys are out of v1 (batch decision 2). `pdf-save`'s
//! `clock.rs::set_mod_date` already writes `/ModDate` (and, for a document
//! with no prior `/Info`, `/CreationDate`) on every save; this module does
//! not change that, it gives the rest of the batch (`Command::SetDocumentInfo`,
//! T-168; lazy reads, T-169) a value type to carry instead of raw strings.

use std::fmt;
use std::str::FromStr;

/// A snapshot of a PDF's `/Info` dictionary. Every field is `Option`:
/// `None` means the key is absent from `/Info`, not "present with an empty
/// string" (batch decision 3) — a UI that clears a field and saves must
/// delete the key, never write `()`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<PdfDate>,
    pub mod_date: Option<PdfDate>,
}

/// The relationship of a [`PdfDate`]'s local time to UT (PDF 32000-1:2008
/// §7.9.4's trailing `O` component).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfDateOffset {
    /// `Z` — local time equals UT.
    Utc,
    /// `+HH'mm'` — local time is ahead of UT.
    Plus { hours: u8, minutes: u8 },
    /// `-HH'mm'` — local time is behind UT.
    Minus { hours: u8, minutes: u8 },
}

/// A parsed PDF date string: `D:YYYYMMDDHHmmSSOHH'mm'` (PDF 32000-1:2008
/// §7.9.4).
///
/// A value type, not a raw `String` — `pdf-save`'s `clock.rs` today only
/// *formats* this string (stamping `/ModDate`/`/CreationDate` on save); it
/// never had to parse one back, because nothing exposed an existing value to
/// a caller. An editable "creation date" field needs to show what is already
/// in the file, so parsing is new work this type exists for (batch decision
/// 4). [`Self::to_pdf_string`] always normalizes to the fully-qualified form
/// — the same shape `clock.rs` already writes — so round-tripping a value
/// through parse then format reproduces it exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfDate {
    pub year: u16,
    /// 1-12.
    pub month: u8,
    /// 1-31 (no calendar/leap-year validation — this is a string format, not
    /// a calendar library; a nonsensical but well-formed date like
    /// 2026-02-30 parses and round-trips like any other).
    pub day: u8,
    /// 0-23.
    pub hour: u8,
    /// 0-59.
    pub minute: u8,
    /// 0-59.
    pub second: u8,
    pub offset: PdfDateOffset,
}

/// Why [`PdfDate::parse`] rejected an input.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PdfDateParseError {
    /// Missing the required `D:` prefix.
    MissingPrefix,
    /// The four-digit year is missing or not all digits.
    InvalidYear,
    /// A component (named) was present but out of range, e.g. month `13`.
    InvalidComponent(&'static str),
    /// The trailing `O`/`HH'mm'` offset is malformed.
    InvalidOffset,
    /// Extra, unrecognized characters after a well-formed date.
    TrailingCharacters,
}

impl fmt::Display for PdfDateParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PdfDateParseError::MissingPrefix => write!(f, "PDF date must start with \"D:\""),
            PdfDateParseError::InvalidYear => write!(f, "invalid or missing 4-digit year"),
            PdfDateParseError::InvalidComponent(name) => {
                write!(f, "invalid value for {name}")
            }
            PdfDateParseError::InvalidOffset => write!(f, "invalid UT offset"),
            PdfDateParseError::TrailingCharacters => {
                write!(f, "unrecognized characters after the date")
            }
        }
    }
}

impl std::error::Error for PdfDateParseError {}

/// Reads a two-digit component starting at `*pos`, without advancing `*pos`
/// if the next two characters are not both ASCII digits — per §7.9.4, every
/// component after the year is optional, but if present, everything before
/// it must also be present. The caller relies on that: once one component
/// comes back `None`, `*pos` has not moved, so every later call in the same
/// sequence also sees non-digits at that position and returns `None` too.
fn take_two_digit_component(
    rest: &str,
    pos: &mut usize,
    min: u8,
    max: u8,
    name: &'static str,
) -> Result<Option<u8>, PdfDateParseError> {
    let bytes = rest.as_bytes();
    let start = *pos;
    if start + 2 > bytes.len() || !bytes[start..start + 2].iter().all(u8::is_ascii_digit) {
        return Ok(None);
    }
    let value: u8 = rest[start..start + 2]
        .parse()
        .map_err(|_| PdfDateParseError::InvalidComponent(name))?;
    if value < min || value > max {
        return Err(PdfDateParseError::InvalidComponent(name));
    }
    *pos = start + 2;
    Ok(Some(value))
}

impl PdfDate {
    /// Parses a PDF date string per PDF 32000-1:2008 §7.9.4.
    ///
    /// Every component after the year defaults per the spec when absent —
    /// month/day default to `1`, hour/minute/second default to `0` — and a
    /// wholly absent offset defaults to [`PdfDateOffset::Utc`].
    pub fn parse(input: &str) -> Result<Self, PdfDateParseError> {
        let rest = input
            .strip_prefix("D:")
            .ok_or(PdfDateParseError::MissingPrefix)?;
        let bytes = rest.as_bytes();
        if bytes.len() < 4 || !bytes[..4].iter().all(u8::is_ascii_digit) {
            return Err(PdfDateParseError::InvalidYear);
        }
        let year: u16 = rest[..4]
            .parse()
            .map_err(|_| PdfDateParseError::InvalidYear)?;
        let mut pos = 4;

        let month = take_two_digit_component(rest, &mut pos, 1, 12, "month")?.unwrap_or(1);
        let day = take_two_digit_component(rest, &mut pos, 1, 31, "day")?.unwrap_or(1);
        let hour = take_two_digit_component(rest, &mut pos, 0, 23, "hour")?.unwrap_or(0);
        let minute = take_two_digit_component(rest, &mut pos, 0, 59, "minute")?.unwrap_or(0);
        let second = take_two_digit_component(rest, &mut pos, 0, 59, "second")?.unwrap_or(0);

        let offset = match bytes.get(pos) {
            None => PdfDateOffset::Utc,
            Some(b'Z') => {
                pos += 1;
                PdfDateOffset::Utc
            }
            Some(sign @ (b'+' | b'-')) => {
                let sign = *sign;
                pos += 1;
                let hours = take_two_digit_component(rest, &mut pos, 0, 23, "offset hour")?
                    .ok_or(PdfDateParseError::InvalidOffset)?;
                if bytes.get(pos) != Some(&b'\'') {
                    return Err(PdfDateParseError::InvalidOffset);
                }
                pos += 1;
                let minutes = take_two_digit_component(rest, &mut pos, 0, 59, "offset minute")?
                    .ok_or(PdfDateParseError::InvalidOffset)?;
                if bytes.get(pos) != Some(&b'\'') {
                    return Err(PdfDateParseError::InvalidOffset);
                }
                pos += 1;
                if sign == b'+' {
                    PdfDateOffset::Plus { hours, minutes }
                } else {
                    PdfDateOffset::Minus { hours, minutes }
                }
            }
            Some(_) => return Err(PdfDateParseError::InvalidOffset),
        };

        if pos != bytes.len() {
            return Err(PdfDateParseError::TrailingCharacters);
        }

        Ok(PdfDate {
            year,
            month,
            day,
            hour,
            minute,
            second,
            offset,
        })
    }

    /// Formats this date back to its canonical `D:YYYYMMDDHHmmSSOHH'mm'`
    /// form — the same shape `pdf-save`'s `clock.rs` writes, so a value
    /// parsed from a file and re-serialized without edits reproduces the
    /// original bytes.
    pub fn to_pdf_string(&self) -> String {
        let offset = match self.offset {
            PdfDateOffset::Utc => "Z".to_string(),
            PdfDateOffset::Plus { hours, minutes } => format!("+{hours:02}'{minutes:02}'"),
            PdfDateOffset::Minus { hours, minutes } => format!("-{hours:02}'{minutes:02}'"),
        };
        format!(
            "D:{:04}{:02}{:02}{:02}{:02}{:02}{offset}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

impl FromStr for PdfDate {
    type Err = PdfDateParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for PdfDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_pdf_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_info_defaults_to_all_absent_keys() {
        let info = DocumentInfo::default();
        assert_eq!(info.title, None);
        assert_eq!(info.mod_date, None);
    }

    #[test]
    fn parses_the_canonical_form_clock_rs_writes() {
        let parsed = PdfDate::parse("D:20260713000000Z").unwrap();
        assert_eq!(
            parsed,
            PdfDate {
                year: 2026,
                month: 7,
                day: 13,
                hour: 0,
                minute: 0,
                second: 0,
                offset: PdfDateOffset::Utc,
            }
        );
    }

    #[test]
    fn formatting_the_canonical_form_reproduces_the_same_bytes() {
        let date = PdfDate::parse("D:20260713000000Z").unwrap();
        assert_eq!(date.to_pdf_string(), "D:20260713000000Z");
    }

    #[test]
    fn parses_a_positive_offset() {
        let parsed = PdfDate::parse("D:20260713153045+05'30'").unwrap();
        assert_eq!(
            parsed.offset,
            PdfDateOffset::Plus {
                hours: 5,
                minutes: 30
            }
        );
        assert_eq!(parsed.to_pdf_string(), "D:20260713153045+05'30'");
    }

    #[test]
    fn parses_a_negative_offset() {
        let parsed = PdfDate::parse("D:20260713153045-08'00'").unwrap();
        assert_eq!(
            parsed.offset,
            PdfDateOffset::Minus {
                hours: 8,
                minutes: 0
            }
        );
        assert_eq!(parsed.to_pdf_string(), "D:20260713153045-08'00'");
    }

    #[test]
    fn missing_components_default_per_spec() {
        // Year-only, per §7.9.4's defaults: month/day -> 01, time -> 00:00:00.
        let parsed = PdfDate::parse("D:2026").unwrap();
        assert_eq!(
            parsed,
            PdfDate {
                year: 2026,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0,
                offset: PdfDateOffset::Utc,
            }
        );
    }

    #[test]
    fn missing_offset_defaults_to_utc() {
        let parsed = PdfDate::parse("D:20260713153045").unwrap();
        assert_eq!(parsed.offset, PdfDateOffset::Utc);
        assert_eq!(parsed.to_pdf_string(), "D:20260713153045Z");
    }

    #[test]
    fn round_trip_holds_for_every_offset_kind() {
        let utc = PdfDate {
            year: 1999,
            month: 12,
            day: 31,
            hour: 23,
            minute: 59,
            second: 59,
            offset: PdfDateOffset::Utc,
        };
        let plus = PdfDate {
            offset: PdfDateOffset::Plus {
                hours: 9,
                minutes: 15,
            },
            ..utc
        };
        let minus = PdfDate {
            offset: PdfDateOffset::Minus {
                hours: 3,
                minutes: 45,
            },
            ..utc
        };

        for date in [utc, plus, minus] {
            let round_tripped = PdfDate::parse(&date.to_pdf_string()).unwrap();
            assert_eq!(round_tripped, date, "{date:?} did not survive a round trip");
        }
    }

    #[test]
    fn rejects_a_missing_prefix() {
        assert_eq!(
            PdfDate::parse("20260713000000Z"),
            Err(PdfDateParseError::MissingPrefix)
        );
    }

    #[test]
    fn rejects_an_out_of_range_month() {
        assert_eq!(
            PdfDate::parse("D:20261300000000Z"),
            Err(PdfDateParseError::InvalidComponent("month"))
        );
    }

    #[test]
    fn rejects_an_out_of_range_day() {
        assert_eq!(
            PdfDate::parse("D:20260732000000Z"),
            Err(PdfDateParseError::InvalidComponent("day"))
        );
    }

    #[test]
    fn rejects_a_malformed_offset_missing_the_closing_quote() {
        assert_eq!(
            PdfDate::parse("D:20260713000000+0530"),
            Err(PdfDateParseError::InvalidOffset)
        );
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert_eq!(
            PdfDate::parse("D:20260713000000Zxyz"),
            Err(PdfDateParseError::TrailingCharacters)
        );
    }

    #[test]
    fn from_str_matches_parse() {
        assert_eq!(
            "D:20260713000000Z".parse::<PdfDate>(),
            PdfDate::parse("D:20260713000000Z")
        );
    }

    #[test]
    fn display_matches_to_pdf_string() {
        let date = PdfDate::parse("D:20260713000000Z").unwrap();
        assert_eq!(date.to_string(), date.to_pdf_string());
    }
}
