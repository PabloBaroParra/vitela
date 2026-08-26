//! Content-stream tokenizer (T-152, first half).
//!
//! Turns raw content-stream bytes into a flat list of operations, each
//! carrying the **byte span it occupies in the source**. Those spans are the
//! whole reason this exists rather than `lopdf::content::Content::decode`:
//! [`crate::edit`] must be able to replace one operand and leave every other
//! byte of the stream exactly as it found it, and a decode/re-encode round
//! trip through lopdf would reformat numbers and whitespace across the
//! entire page.

use crate::error::EditError;
use std::ops::Range;

/// A value appearing as an operand in a content stream.
///
/// Deliberately not `lopdf::Object`: that type also models indirect
/// references and streams, neither of which can occur inside a content
/// stream, and constructing it would mean giving up the byte spans this
/// module exists to produce.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Integer(i64),
    Real(f64),
    /// A `/Name`, with `#xx` escapes already resolved.
    Name(String),
    /// A `(literal string)`, with escapes already resolved. Kept as bytes:
    /// the codes mean nothing until they are read through the active font's
    /// encoding, which is [`crate::encoding`]'s job.
    LiteralString(Vec<u8>),
    /// A `<hex string>`, decoded to the same kind of raw code bytes.
    HexString(Vec<u8>),
    Array(Vec<Operand>),
    /// A `<< ... >>` dictionary, as ordered key/value pairs — only ever
    /// appears on marked-content and inline-image operators, which this
    /// crate passes through rather than interprets.
    Dictionary(Vec<(String, Operand)>),
    Boolean(bool),
    Null,
}

impl Operand {
    /// The numeric value of an `Integer` or `Real`, for the operators whose
    /// operands are coordinates and matrices.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Operand::Integer(value) => Some(*value as f64),
            Operand::Real(value) => Some(*value),
            _ => None,
        }
    }

    /// The raw code bytes of either string form — the two are
    /// interchangeable as show-text operands.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Operand::LiteralString(bytes) | Operand::HexString(bytes) => Some(bytes),
            _ => None,
        }
    }
}

/// One operator with its operands, and where both live in the source bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedOperation {
    pub operator: String,
    pub operands: Vec<Operand>,
    /// Covers the first operand through the last byte of the operator — the
    /// unit to delete or replace wholesale.
    pub span: Range<usize>,
    /// One span per entry in `operands`, in the same order.
    pub operand_spans: Vec<Range<usize>>,
}

/// Splits `bytes` into operations, preserving where each one came from.
pub fn tokenize(bytes: &[u8]) -> Result<Vec<SpannedOperation>, EditError> {
    Lexer { bytes, position: 0 }.run()
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b'\0' | b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

/// `(`, `)`, `<`, `>`, `[`, `]`, `{`, `}`, `/`, `%` end a bare token.
fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn is_regular(byte: u8) -> bool {
    !is_whitespace(byte) && !is_delimiter(byte)
}

struct Lexer<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Lexer<'a> {
    fn run(mut self) -> Result<Vec<SpannedOperation>, EditError> {
        let mut operations = Vec::new();
        // Operands accumulate until an operator consumes all of them: a
        // content stream is postfix, so an operator's arguments are
        // everything read since the previous operator.
        let mut operands: Vec<Operand> = Vec::new();
        let mut operand_spans: Vec<Range<usize>> = Vec::new();

        while let Some(byte) = self.peek() {
            if is_whitespace(byte) {
                self.position += 1;
                continue;
            }
            if byte == b'%' {
                self.skip_comment();
                continue;
            }

            let start = self.position;
            match self.read_operand()? {
                Some(operand) => {
                    operands.push(operand);
                    operand_spans.push(start..self.position);
                }
                None => {
                    let operator = self.read_operator();
                    let span_start = operand_spans.first().map_or(start, |first| first.start);
                    let operation = if operator == "BI" {
                        // The payload is opaque binary; swallow it whole so
                        // the operator stream stays in sync.
                        let end = self.skip_inline_image(start)?;
                        SpannedOperation {
                            operator,
                            operands: std::mem::take(&mut operands),
                            span: span_start..end,
                            operand_spans: std::mem::take(&mut operand_spans),
                        }
                    } else {
                        SpannedOperation {
                            operator,
                            operands: std::mem::take(&mut operands),
                            span: span_start..self.position,
                            operand_spans: std::mem::take(&mut operand_spans),
                        }
                    };
                    operations.push(operation);
                }
            }
        }

        Ok(operations)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn skip_comment(&mut self) {
        while let Some(byte) = self.peek() {
            if byte == b'\n' || byte == b'\r' {
                break;
            }
            self.position += 1;
        }
    }

    fn malformed(&self, reason: &str, offset: usize) -> EditError {
        EditError::MalformedContent {
            reason: reason.to_string(),
            offset,
        }
    }

    /// Reads one operand, or `None` when the next token is an operator
    /// (a bare keyword that is not `true`/`false`/`null`).
    fn read_operand(&mut self) -> Result<Option<Operand>, EditError> {
        let start = self.position;
        match self.peek().expect("caller checked there is a byte") {
            b'/' => Ok(Some(self.read_name())),
            b'(' => Ok(Some(Operand::LiteralString(self.read_literal_string()?))),
            b'[' => Ok(Some(self.read_array()?)),
            b'<' => {
                if self.bytes.get(self.position + 1) == Some(&b'<') {
                    Ok(Some(self.read_dictionary()?))
                } else {
                    Ok(Some(Operand::HexString(self.read_hex_string()?)))
                }
            }
            b']' | b'>' | b')' | b'}' | b'{' => Err(self.malformed("unexpected delimiter", start)),
            byte if byte == b'+' || byte == b'-' || byte == b'.' || byte.is_ascii_digit() => {
                Ok(Some(self.read_number()))
            }
            _ => {
                // A keyword: either a literal or the operator itself. Peek
                // without consuming so `read_operator` can take it.
                match self.peek_keyword() {
                    b"true" => {
                        self.read_operator();
                        Ok(Some(Operand::Boolean(true)))
                    }
                    b"false" => {
                        self.read_operator();
                        Ok(Some(Operand::Boolean(false)))
                    }
                    b"null" => {
                        self.read_operator();
                        Ok(Some(Operand::Null))
                    }
                    _ => Ok(None),
                }
            }
        }
    }

    fn peek_keyword(&self) -> &'a [u8] {
        let mut end = self.position;
        while end < self.bytes.len() && is_regular(self.bytes[end]) {
            end += 1;
        }
        &self.bytes[self.position..end]
    }

    fn read_operator(&mut self) -> String {
        let keyword = self.peek_keyword();
        self.position += keyword.len();
        if keyword.is_empty() {
            // A lone delimiter that is not a valid operand start; consume it
            // so the loop cannot spin forever on malformed input.
            self.position += 1;
        }
        String::from_utf8_lossy(keyword).into_owned()
    }

    fn read_name(&mut self) -> Operand {
        self.position += 1; // the '/'
        let mut name = Vec::new();
        while let Some(byte) = self.peek() {
            if !is_regular(byte) {
                break;
            }
            if byte == b'#' {
                let high = self.bytes.get(self.position + 1).copied();
                let low = self.bytes.get(self.position + 2).copied();
                if let (Some(high), Some(low)) = (high, low) {
                    if let (Some(high), Some(low)) =
                        ((high as char).to_digit(16), (low as char).to_digit(16))
                    {
                        name.push((high * 16 + low) as u8);
                        self.position += 3;
                        continue;
                    }
                }
            }
            name.push(byte);
            self.position += 1;
        }
        Operand::Name(String::from_utf8_lossy(&name).into_owned())
    }

    fn read_number(&mut self) -> Operand {
        let start = self.position;
        let mut is_real = false;
        while let Some(byte) = self.peek() {
            match byte {
                b'0'..=b'9' | b'+' | b'-' => self.position += 1,
                b'.' => {
                    is_real = true;
                    self.position += 1;
                }
                _ => break,
            }
        }
        let text = String::from_utf8_lossy(&self.bytes[start..self.position]);
        if is_real {
            // `4.` and `-.002` are both legal PDF reals; Rust parses both,
            // but a stray form is treated as zero rather than as a hard
            // error, matching how viewers recover.
            Operand::Real(text.parse::<f64>().unwrap_or(0.0))
        } else {
            match text.parse::<i64>() {
                Ok(value) => Operand::Integer(value),
                Err(_) => Operand::Real(text.parse::<f64>().unwrap_or(0.0)),
            }
        }
    }

    fn read_literal_string(&mut self) -> Result<Vec<u8>, EditError> {
        let start = self.position;
        self.position += 1; // the '('
        let mut depth = 1usize;
        let mut out = Vec::new();

        while let Some(byte) = self.peek() {
            self.position += 1;
            match byte {
                b'\\' => {
                    let Some(escaped) = self.peek() else {
                        return Err(self.malformed("unterminated literal string", start));
                    };
                    self.position += 1;
                    match escaped {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        // A backslash before an end-of-line is a line
                        // continuation: it contributes nothing.
                        b'\n' => {}
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.position += 1;
                            }
                        }
                        b'0'..=b'7' => {
                            let mut value = u32::from(escaped - b'0');
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(digit @ b'0'..=b'7') => {
                                        value = value * 8 + u32::from(digit - b'0');
                                        self.position += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(value as u8);
                        }
                        other => out.push(other),
                    }
                }
                b'(' => {
                    depth += 1;
                    out.push(byte);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(out);
                    }
                    out.push(byte);
                }
                other => out.push(other),
            }
        }

        Err(self.malformed("unterminated literal string", start))
    }

    fn read_hex_string(&mut self) -> Result<Vec<u8>, EditError> {
        let start = self.position;
        self.position += 1; // the '<'
        let mut digits = Vec::new();

        while let Some(byte) = self.peek() {
            self.position += 1;
            match byte {
                b'>' => {
                    if digits.len() % 2 == 1 {
                        // "If the final digit is missing, it is assumed to
                        // be 0" — PDF 32000-1 7.3.4.3.
                        digits.push(b'0');
                    }
                    return Ok(digits
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|pair| {
                            let high = (pair[0] as char).to_digit(16).unwrap_or(0) as u8;
                            let low = (pair[1] as char).to_digit(16).unwrap_or(0) as u8;
                            high * 16 + low
                        })
                        .collect());
                }
                byte if byte.is_ascii_hexdigit() => digits.push(byte),
                byte if is_whitespace(byte) => {}
                _ => return Err(self.malformed("invalid digit in hex string", start)),
            }
        }

        Err(self.malformed("unterminated hex string", start))
    }

    fn read_array(&mut self) -> Result<Operand, EditError> {
        let start = self.position;
        self.position += 1; // the '['
        let mut items = Vec::new();

        loop {
            while self.peek().is_some_and(is_whitespace) {
                self.position += 1;
            }
            match self.peek() {
                None => return Err(self.malformed("unterminated array", start)),
                Some(b']') => {
                    self.position += 1;
                    return Ok(Operand::Array(items));
                }
                Some(b'%') => self.skip_comment(),
                Some(_) => match self.read_operand()? {
                    Some(item) => items.push(item),
                    // A bare keyword inside an array is not legal content;
                    // dropping it silently would hide a malformed file.
                    None => return Err(self.malformed("keyword inside array", self.position)),
                },
            }
        }
    }

    fn read_dictionary(&mut self) -> Result<Operand, EditError> {
        let start = self.position;
        self.position += 2; // the '<<'
        let mut entries = Vec::new();

        loop {
            while self.peek().is_some_and(is_whitespace) {
                self.position += 1;
            }
            match self.peek() {
                None => return Err(self.malformed("unterminated dictionary", start)),
                Some(b'>') => {
                    self.position += 1;
                    if self.peek() == Some(b'>') {
                        self.position += 1;
                    }
                    return Ok(Operand::Dictionary(entries));
                }
                Some(b'%') => self.skip_comment(),
                Some(b'/') => {
                    let Operand::Name(key) = self.read_name() else {
                        unreachable!("read_name always returns a Name");
                    };
                    while self.peek().is_some_and(is_whitespace) {
                        self.position += 1;
                    }
                    match self.read_operand()? {
                        Some(value) => entries.push((key, value)),
                        None => return Err(self.malformed("dictionary key without value", start)),
                    }
                }
                Some(_) => return Err(self.malformed("dictionary key is not a name", start)),
            }
        }
    }

    /// Consumes an inline image's dictionary, `ID`, binary payload and `EI`,
    /// returning the offset just past the `EI`.
    ///
    /// The payload has no length prefix, so the end is found the only way
    /// available: the first `EI` that is preceded by whitespace and followed
    /// by whitespace or end-of-stream. That is a heuristic — the same one
    /// viewers use — and it is why inline images are passed through opaquely
    /// rather than edited.
    fn skip_inline_image(&mut self, start: usize) -> Result<usize, EditError> {
        while self.position < self.bytes.len() {
            if self.bytes[self.position..].starts_with(b"ID") {
                self.position += 2;
                // Exactly one whitespace byte separates `ID` from the data.
                if self.peek().is_some_and(is_whitespace) {
                    self.position += 1;
                }
                break;
            }
            self.position += 1;
        }

        while self.position + 1 < self.bytes.len() {
            let is_ei = self.bytes[self.position] == b'E' && self.bytes[self.position + 1] == b'I';
            let preceded_by_space =
                self.position > 0 && is_whitespace(self.bytes[self.position - 1]);
            let followed_by_space = self
                .bytes
                .get(self.position + 2)
                .is_none_or(|&byte| is_whitespace(byte));
            if is_ei && preceded_by_space && followed_by_space {
                self.position += 2;
                return Ok(self.position);
            }
            self.position += 1;
        }

        Err(self.malformed("unterminated inline image", start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(source: &str) -> Vec<SpannedOperation> {
        tokenize(source.as_bytes()).expect("valid content stream")
    }

    fn operators(source: &str) -> Vec<String> {
        ops(source).into_iter().map(|op| op.operator).collect()
    }

    #[test]
    fn reads_operators_in_stream_order() {
        assert_eq!(
            operators("BT /F1 12 Tf (Hi) Tj ET"),
            vec!["BT", "Tf", "Tj", "ET"]
        );
    }

    #[test]
    fn each_operation_carries_its_operands() {
        let operations = ops("/F1 12 Tf");

        assert_eq!(operations.len(), 1);
        assert_eq!(
            operations[0].operands,
            vec![Operand::Name("F1".to_string()), Operand::Integer(12)]
        );
    }

    /// The property `edit` depends on: an operand's span must slice the
    /// source back to exactly that operand's bytes, so a replacement can be
    /// spliced in without disturbing its neighbours.
    #[test]
    fn an_operand_span_slices_back_to_that_operand() {
        let source = "BT /F1 12 Tf (Hello) Tj ET";
        let operations = ops(source);
        let show_text = &operations[2];

        assert_eq!(show_text.operator, "Tj");
        let span = show_text.operand_spans[0].clone();
        assert_eq!(&source.as_bytes()[span], b"(Hello)");
    }

    /// And the operation span must cover its operands *and* its operator,
    /// which is how a whole `cm` gets rewritten or a whole item deleted.
    #[test]
    fn an_operation_span_covers_its_operands_and_operator() {
        let source = "q 1 0 0 1 5 5 cm /Im1 Do Q";
        let operations = ops(source);
        let concatenate = &operations[1];

        assert_eq!(concatenate.operator, "cm");
        assert_eq!(
            &source.as_bytes()[concatenate.span.clone()],
            b"1 0 0 1 5 5 cm"
        );
    }

    #[test]
    fn an_operation_with_no_operands_spans_just_its_operator() {
        let source = "BT ET";
        let operations = ops(source);

        assert_eq!(&source.as_bytes()[operations[0].span.clone()], b"BT");
        assert_eq!(&source.as_bytes()[operations[1].span.clone()], b"ET");
    }

    #[test]
    fn reads_the_number_forms_pdf_allows() {
        let operations = ops("12 -3 1.5 -.002 4. 0 d0");

        assert_eq!(operations[0].operands[0], Operand::Integer(12));
        assert_eq!(operations[0].operands[1], Operand::Integer(-3));
        assert_eq!(operations[0].operands[2], Operand::Real(1.5));
        assert_eq!(operations[0].operands[3], Operand::Real(-0.002));
        assert_eq!(operations[0].operands[4], Operand::Real(4.0));
    }

    #[test]
    fn decodes_literal_string_escapes() {
        let operations = ops(r"(a\(b\)c\\d\ne) Tj");

        assert_eq!(
            operations[0].operands[0],
            Operand::LiteralString(b"a(b)c\\d\ne".to_vec())
        );
    }

    /// Unescaped parentheses nest, and only the balancing one closes the
    /// string — a tokenizer that stops at the first `)` would truncate the
    /// run and then corrupt the stream when writing it back.
    #[test]
    fn literal_strings_nest_balanced_parentheses() {
        let operations = ops("((inner) outer) Tj");

        assert_eq!(
            operations[0].operands[0],
            Operand::LiteralString(b"(inner) outer".to_vec())
        );
    }

    #[test]
    fn decodes_octal_escapes_in_literal_strings() {
        let operations = ops(r"(\101\102) Tj");

        assert_eq!(
            operations[0].operands[0],
            Operand::LiteralString(b"AB".to_vec())
        );
    }

    /// A backslash before an end-of-line means "this string continues", and
    /// neither the backslash nor the newline is part of the content.
    #[test]
    fn a_line_continuation_contributes_no_bytes() {
        let operations = ops("(one\\\ntwo) Tj");

        assert_eq!(
            operations[0].operands[0],
            Operand::LiteralString(b"onetwo".to_vec())
        );
    }

    #[test]
    fn decodes_hex_strings_padding_an_odd_final_digit() {
        assert_eq!(
            ops("<48656C6C6F> Tj")[0].operands[0],
            Operand::HexString(b"Hello".to_vec())
        );
        assert_eq!(
            ops("<4A5> Tj")[0].operands[0],
            Operand::HexString(vec![0x4A, 0x50]),
            "an odd trailing digit is padded with zero, per the spec"
        );
    }

    #[test]
    fn decodes_hash_escapes_in_names() {
        assert_eq!(
            ops("/A#20B Do")[0].operands[0],
            Operand::Name("A B".to_string())
        );
    }

    #[test]
    fn reads_arrays_as_a_single_operand() {
        let operations = ops("[(A) -20 (B)] TJ");

        assert_eq!(operations[0].operator, "TJ");
        assert_eq!(
            operations[0].operands[0],
            Operand::Array(vec![
                Operand::LiteralString(b"A".to_vec()),
                Operand::Integer(-20),
                Operand::LiteralString(b"B".to_vec()),
            ])
        );
    }

    #[test]
    fn reads_dictionaries_as_a_single_operand() {
        let operations = ops("/OC <</Type /Foo>> BDC");

        assert_eq!(operations[0].operator, "BDC");
        assert_eq!(
            operations[0].operands[1],
            Operand::Dictionary(vec![("Type".to_string(), Operand::Name("Foo".to_string()))])
        );
    }

    #[test]
    fn skips_comments() {
        assert_eq!(
            operators("BT % this is a comment ) ( \n ET"),
            vec!["BT", "ET"]
        );
    }

    #[test]
    fn reads_booleans_and_null() {
        let operations = ops("true false null gs");

        assert_eq!(operations[0].operands[0], Operand::Boolean(true));
        assert_eq!(operations[0].operands[1], Operand::Boolean(false));
        assert_eq!(operations[0].operands[2], Operand::Null);
    }

    /// Inline-image data is arbitrary binary that can contain anything —
    /// `)`, `EI`, an entire fake operator. Tokenizing it as if it were
    /// operators would desynchronize the rest of the page, so the whole
    /// `BI`..`EI` run is taken as one opaque operation.
    #[test]
    fn an_inline_image_is_one_opaque_operation() {
        let source: &[u8] = b"q BI /W 2 /H 2 ID \x00(Tj\xff\xfe EI Q";
        let operations = tokenize(source).expect("valid content stream");

        assert_eq!(
            operations
                .iter()
                .map(|op| op.operator.as_str())
                .collect::<Vec<_>>(),
            vec!["q", "BI", "Q"],
            "the binary payload must not produce operators of its own"
        );
        assert!(
            String::from_utf8_lossy(&source[operations[1].span.clone()]).ends_with("EI"),
            "the span must cover the whole inline image"
        );
    }

    #[test]
    fn an_unterminated_string_is_reported_with_its_offset() {
        let error = tokenize(b"BT (never closed Tj").expect_err("must not be accepted");

        assert!(matches!(
            error,
            EditError::MalformedContent { offset, .. } if offset == 3
        ));
    }

    #[test]
    fn an_unterminated_array_is_reported() {
        let error = tokenize(b"[(A) -20 TJ").expect_err("must not be accepted");

        assert!(matches!(error, EditError::MalformedContent { .. }));
    }

    #[test]
    fn an_empty_stream_yields_no_operations() {
        assert!(ops("").is_empty());
        assert!(ops("   \n\t  ").is_empty());
    }
}
