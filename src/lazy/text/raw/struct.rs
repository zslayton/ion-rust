#![allow(non_camel_case_types)]

use std::ops::Range;

use nom::character::streaming::satisfy;

use crate::lazy::decoder::private::{LazyContainerPrivate, LazyRawFieldPrivate};
use crate::lazy::decoder::{
    LazyRawField, LazyRawFieldExpr, LazyRawStruct, LazyRawValue, RawFieldExpr, RawValueExpr,
};
use crate::lazy::encoding::TextEncoding_1_0;
use crate::lazy::text::buffer::TextBufferView;
use crate::lazy::text::parse_result::{AddContext, ToIteratorOutput};
use crate::lazy::text::value::{LazyRawTextValue_1_0, RawTextAnnotationsIterator};
use crate::{IonResult, RawSymbolTokenRef};

#[derive(Clone, Copy, Debug)]
pub struct RawTextStructIterator_1_0<'top> {
    input: TextBufferView<'top>,
    has_returned_error: bool,
}

impl<'top> RawTextStructIterator_1_0<'top> {
    pub(crate) fn new(input: TextBufferView<'top>) -> Self {
        RawTextStructIterator_1_0 {
            input,
            has_returned_error: false,
        }
    }

    pub(crate) fn find_span(&self) -> IonResult<Range<usize>> {
        // The input has already skipped past the opening delimiter.
        let start = self.input.offset() - 1;
        // We need to find the input slice containing the closing delimiter. It's either...
        let input_after_last = if let Some(field_result) = self.last() {
            let (_name, RawValueExpr::ValueLiteral(value)) = field_result?.into_name_value() else {
                unreachable!("struct field with macro invocation in Ion 1.0");
            };
            // ...the input slice that follows the last field...
            value
                .matched
                .input
                .slice_to_end(value.matched.encoded_value.total_length())
        } else {
            // ...or there aren't fields, so it's just the input after the opening delimiter.
            self.input
        };
        let (mut input_after_ws, _ws) =
            input_after_last
                .match_optional_comments_and_whitespace()
                .with_context("seeking the end of a struct", input_after_last)?;
        // Skip an optional comma and more whitespace
        if input_after_ws.bytes().first() == Some(&b',') {
            (input_after_ws, _) = input_after_ws
                .slice_to_end(1)
                .match_optional_comments_and_whitespace()
                .with_context("skipping a list's trailing comma", input_after_ws)?;
        }
        let (input_after_end, _end_delimiter) = satisfy(|c| c == b'}' as char)(input_after_ws)
            .with_context("seeking the closing delimiter of a struct", input_after_ws)?;
        let end = input_after_end.offset();
        Ok(start..end)
    }
}

impl<'top> Iterator for RawTextStructIterator_1_0<'top> {
    type Item = IonResult<LazyRawFieldExpr<'top, TextEncoding_1_0>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.has_returned_error {
            return None;
        }
        match self.input.match_struct_field() {
            Ok((remaining_input, Some(field))) => {
                self.input = remaining_input;
                Some(Ok(RawFieldExpr::new(
                    field.name(),
                    RawValueExpr::ValueLiteral(field.value),
                )))
            }
            Ok((_, None)) => None,
            Err(e) => {
                self.has_returned_error = true;
                e.with_context("reading the next struct field", self.input)
                    .transpose()
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LazyRawTextField_1_0<'top> {
    pub(crate) value: LazyRawTextValue_1_0<'top>,
}

impl<'top> LazyRawTextField_1_0<'top> {
    pub(crate) fn new(value: LazyRawTextValue_1_0<'top>) -> Self {
        LazyRawTextField_1_0 { value }
    }

    pub fn name(&self) -> RawSymbolTokenRef<'top> {
        let encoded_value = self.value.matched.encoded_value;
        let matched = &self.value.matched;
        let allocator = matched.input.allocator;
        // We're in a struct field, the field name _must_ be populated.
        // If it's not (or the field name is not a valid SID or UTF-8 string despite matching),
        // that's a bug. We can safely unwrap/expect here.
        let matched_symbol = encoded_value
            .field_name_syntax()
            .expect("field name syntax not available");
        let name_length = encoded_value
            .field_name_range()
            .expect("field name length not available")
            .len();
        matched_symbol
            .read(allocator, matched.input.slice(0, name_length))
            .expect("invalid struct field name")
    }

    pub fn value(&self) -> LazyRawTextValue_1_0<'top> {
        self.value
    }

    pub(crate) fn into_value(self) -> LazyRawTextValue_1_0<'top> {
        self.value
    }
}

impl<'top> LazyRawFieldPrivate<'top, TextEncoding_1_0> for LazyRawTextField_1_0<'top> {
    fn into_value(self) -> LazyRawTextValue_1_0<'top> {
        self.value
    }

    fn input_span(&self) -> &[u8] {
        self.value.matched.input.bytes()
    }

    fn input_offset(&self) -> usize {
        self.value.matched.input.offset()
    }
}

impl<'top> LazyRawField<'top, TextEncoding_1_0> for LazyRawTextField_1_0<'top> {
    fn name(&self) -> RawSymbolTokenRef<'top> {
        LazyRawTextField_1_0::name(self)
    }

    fn value(&self) -> LazyRawTextValue_1_0<'top> {
        self.value()
    }

    fn name_range(&self) -> Range<usize> {
        self.value.matched.encoded_value.field_name_range().unwrap()
    }

    fn name_span(&self) -> &[u8] {
        let stream_range = self.name_range();
        let input_buffer = &self.value.matched.input;
        let offset = input_buffer.offset();
        let local_range = (stream_range.start - offset)..(stream_range.end - offset);
        input_buffer
            .bytes()
            .get(local_range)
            .expect("field name bytes not in buffer")
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LazyRawTextStruct_1_0<'top> {
    pub(crate) value: LazyRawTextValue_1_0<'top>,
}

impl<'top> LazyContainerPrivate<'top, TextEncoding_1_0> for LazyRawTextStruct_1_0<'top> {
    fn from_value(value: LazyRawTextValue_1_0<'top>) -> Self {
        LazyRawTextStruct_1_0 { value }
    }
}

impl<'top> LazyRawStruct<'top, TextEncoding_1_0> for LazyRawTextStruct_1_0<'top> {
    type Iterator = RawTextStructIterator_1_0<'top>;

    fn annotations(&self) -> RawTextAnnotationsIterator<'top> {
        self.value.annotations()
    }

    fn iter(&self) -> Self::Iterator {
        let open_brace_index =
            self.value.matched.encoded_value.data_offset() - self.value.matched.input.offset();
        // Slice the input to skip the opening `{`
        RawTextStructIterator_1_0::new(self.value.matched.input.slice_to_end(open_brace_index + 1))
    }
}

impl<'top> IntoIterator for LazyRawTextStruct_1_0<'top> {
    type Item = IonResult<LazyRawFieldExpr<'top, TextEncoding_1_0>>;
    type IntoIter = RawTextStructIterator_1_0<'top>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use crate::lazy::decoder::{LazyRawReader, LazyRawStruct, LazyRawValue};
    use crate::lazy::text::raw::reader::LazyRawTextReader_1_0;
    use crate::lazy::text::raw::v1_1::reader::LazyRawTextReader_1_1;
    use crate::IonResult;
    use bumpalo::Bump as BumpAllocator;

    fn expect_struct_range(ion_data: &str, expected: Range<usize>) -> IonResult<()> {
        let allocator = BumpAllocator::new();
        let reader = &mut LazyRawTextReader_1_0::new(ion_data.as_bytes());
        let value = reader.next(&allocator)?.expect_value()?;
        let actual_range = value.matched.encoded_value.data_range();
        assert_eq!(
            actual_range, expected,
            "Struct range ({:?}) did not match expected range ({:?})",
            actual_range, expected
        );
        println!("input ok: {}", ion_data);
        Ok(())
    }

    #[test]
    fn struct_range() -> IonResult<()> {
        // For each pair below, we'll confirm that the top-level struct is found to
        // occupy the specified input range.
        let tests = &[
            // (Ion input, expected range of the struct)
            ("{}", 0..2),
            ("  {}  ", 2..4),
            ("{a:1}", 0..5),
            ("{a: 1}", 0..6),
            ("{a: 1, b: 2}", 0..12),
            ("{a: 1, /* comment }}} */ b: 2}", 0..30),
            // Nested
            ("{a: 1, b: 2, c: {d: 3, e: 4, f: 5}, g: 6}", 0..41),
            // Doubly nested
            ("{a: 1, b: 2, c: {d: 3, e: {foo: bar}, f: 5}, g: 6}", 0..50),
        ];
        for test in tests {
            expect_struct_range(test.0, test.1.clone())?;
        }
        Ok(())
    }

    // #[test]
    // Clippy thinks a slice with a single range inside is likely to be a mistake, but in this
    // test it's intentional.
    #[allow(clippy::single_range_in_vec_init)]
    fn field_name_ranges() -> IonResult<()> {
        // For each pair below, we'll confirm that the top-level struct's field names are found to
        // occupy the specified input ranges.
        let tests: &[(&str, &[Range<usize>])] = &[
            // (Ion input, expected ranges of the struct's field names)
            ("{a:1}", &[1..2]),
            ("{a: 1}", &[1..2]),
            ("{a: 1, b: 2}", &[1..2, 7..8]),
            ("{a: 1, /* comment }}} */ b: 2}", &[1..2, 24..25]),
            ("{ a: /* comment */ 1, b: 2}", &[2..3, 22..23]),
            (
                "{a: 1, b: 2, c: {d: 3, e: 4, f: 5}, g: 6}",
                &[1..2, 7..8, 13..14, 36..37],
            ),
        ];
        for (input, field_name_ranges) in tests {
            let bump = bumpalo::Bump::new();
            let mut reader = LazyRawTextReader_1_1::new(input.as_bytes());
            let struct_ = reader
                .next(&bump)?
                .expect_value()?
                .read()?
                .expect_struct()?;
            for (_field_result, _range) in struct_.iter().zip(field_name_ranges.iter()) {
                // let = field_result?.expect_name_value()?;
                // assert_eq!(field.na)
                todo!()
            }
        }
        Ok(())
    }
}
