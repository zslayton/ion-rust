// Copyright Amazon.com, Inc. or its affiliates.

//! Provides APIs to read Ion data into [Element] from different sources such
//! as slices or files.

use crate::element::{Annotations, Element, Sequence, Struct, Value};
use crate::result::{decoding_error, IonResult};
use crate::value_reader::{SequenceRef, StructRef, ValueReader};
use crate::{IonReader, RawIonReader, UserReader, ValueRef};

/// Reads Ion data into [`Element`] instances.
///
/// This trait is automatically implemented by all Ion reader implementations that operate
/// at the highest layer of abstraction, sometimes called the 'user' layer.
pub trait ElementReader {
    type ElementIter<'a>: Iterator<Item = IonResult<Element>>
    where
        Self: 'a;

    /// Recursively materializes the next Ion value, returning it as an `Ok(Element)`.
    /// If there is no more data left to be read, returns `Ok(None)`.
    /// If an error occurs while the data is being read, returns `Err(IonError)`.
    fn read_next_element(&mut self) -> IonResult<Option<Element>>;

    /// Returns an iterator over the [Element]s in the data stream.
    fn elements(&mut self) -> Self::ElementIter<'_>;

    /// Like [Self::read_next_element], this method reads the next Ion value in the input stream,
    /// returning it as an `Ok(Element)`. However, it also requires that the stream contain exactly
    /// one value.
    ///
    /// If the stream's data is valid and it contains one value, returns `Ok(Element)`.
    /// If the stream's data is invalid or the stream does not contain exactly one value,
    /// returns `Err(IonError)`.
    fn read_one_element(&mut self) -> IonResult<Element> {
        let mut iter = self.elements();
        let only_element = match iter.next() {
            Some(Ok(element)) => element,
            Some(Err(e)) => return Err(e),
            None => return decoding_error("expected 1 value, found 0"),
        };
        // See if there is a second, unexpected value.
        match iter.next() {
            Some(Ok(element)) => {
                return decoding_error(format!(
                    "found more than one value; second value: {}",
                    element
                ))
            }
            Some(Err(e)) => return decoding_error(format!("error after expected value: {}", e)),
            None => {}
        };
        Ok(only_element)
    }

    /// Reads all of the values in the input stream, materializing each into an [Element] and
    /// returning the complete sequence as a `Vec<Element>`.
    ///
    /// If an error occurs while reading, returns `Err(IonError)`.
    fn read_all_elements(&mut self) -> IonResult<Vec<Element>> {
        self.elements().collect()
    }
}

impl<R> ElementReader for UserReader<R>
where
    R: RawIonReader,
{
    type ElementIter<'a> = ElementIterator<'a, UserReader<R>> where R: 'a;

    fn read_next_element(&mut self) -> IonResult<Option<Element>> {
        ElementLoader::for_reader(self).materialize_next()
    }

    fn elements(&mut self) -> Self::ElementIter<'_> {
        ElementIterator { reader: self }
    }
}

/// Holds a reference to a given [ElementReader] implementation and yields one [Element] at a time
/// until the stream is exhausted or invalid data is encountered.
pub struct ElementIterator<'a, R: ElementReader> {
    reader: &'a mut R,
}

impl<'a, R: ElementReader> Iterator for ElementIterator<'a, R> {
    type Item = IonResult<Element>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.read_next_element() {
            Ok(Some(element)) => Some(Ok(element)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        }
    }
}

/// Helper type; wraps an [ElementReader] and recursively materializes the next value in the
/// reader's input, reporting any errors that might occur along the way.
struct ElementLoader<'a, R: RawIonReader> {
    reader: &'a mut UserReader<R>,
}

impl<'a, R: RawIonReader + 'a> ElementLoader<'a, R> {
    pub(crate) fn for_reader(reader: &mut UserReader<R>) -> ElementLoader<R> {
        ElementLoader { reader }
    }

    /// Advances the reader to the next value in the stream and uses [Self::materialize_value]
    /// to materialize it.
    pub(crate) fn materialize_next(&mut self) -> IonResult<Option<Element>> {
        // Advance the reader to the next value
        self.reader
            .next()?
            .map(|mut value_reader| Self::materialize_value(&mut value_reader))
            .transpose()
    }

    /// Recursively materialize the reader's current Ion value and returns it as `Ok(Some(value))`.
    /// If there are no more values at this level, returns `Ok(None)`.
    /// If an error occurs while materializing the value, returns an `Err`.
    /// Calling this method advances the reader and consumes the current value.
    fn materialize_value(value_reader: &mut ValueReader<R>) -> IonResult<Element> {
        // Collect this item's annotations into a Vec. We have to do this before materializing the
        // value itself because materializing a collection requires advancing the reader further.
        let mut annotations = Vec::new();
        for annotation in value_reader.annotations() {
            annotations.push(annotation?.to_owned());
        }

        let value = match value_reader.read()? {
            ValueRef::Null(ion_type) => Value::Null(ion_type),
            ValueRef::Bool(b) => Value::Bool(b),
            ValueRef::Int(i) => Value::Int(i),
            ValueRef::Float(f) => Value::Float(f),
            ValueRef::Decimal(d) => Value::Decimal(d),
            ValueRef::Timestamp(t) => Value::Timestamp(t),
            ValueRef::String(s) => Value::String(s.into()),
            ValueRef::Symbol(s) => Value::Symbol(s),
            ValueRef::Blob(b) => Value::Blob(b.into()),
            ValueRef::Clob(c) => Value::Clob(c.into()),
            ValueRef::SExp(s) => Value::SExp(Self::materialize_sequence(s)?),
            ValueRef::List(l) => Value::List(Self::materialize_sequence(l)?),
            ValueRef::Struct(s) => Value::Struct(Self::materialize_struct(s)?),
        };

        Ok(Element::new(Annotations::new(annotations), value))
    }

    /// Steps into the current sequence and materializes each of its children to construct
    /// an [Vec<Element>]. When all of the the children have been materialized, steps out.
    /// The reader MUST be positioned over a list or s-expression when this is called.
    fn materialize_sequence(sequence: SequenceRef<R>) -> IonResult<Sequence> {
        let mut child_elements = Vec::new();

        let mut seq_reader = sequence.reader()?;
        while let Some(mut value) = seq_reader.next_element()? {
            child_elements.push(Self::materialize_value(&mut value)?);
        }
        Ok(child_elements.into())
    }

    /// Steps into the current struct and materializes each of its fields to construct
    /// an [OwnedStruct]. When all of the the fields have been materialized, steps out.
    /// The reader MUST be positioned over a struct when this is called.
    fn materialize_struct(struct_ref: StructRef<R>) -> IonResult<Struct> {
        let mut child_elements = Vec::new();
        let mut struct_reader = struct_ref.reader()?;
        while let Some(mut field) = struct_reader.next_field()? {
            child_elements.push((
                field.read_name()?.to_owned(),
                Self::materialize_value(&mut field.value())?,
            ))
        }
        Ok(Struct::from_iter(child_elements.into_iter()))
    }
}

#[cfg(test)]
mod reader_tests {
    use super::*;
    use crate::element::builders::{ion_list, ion_sexp, ion_struct};
    use crate::element::Value::*;
    use crate::element::{Element, IntoAnnotatedElement};
    use crate::ion_eq::IonEq;
    use crate::types::integer::Int;
    use crate::types::timestamp::Timestamp as TS;
    use crate::{IonType, Symbol};
    use bigdecimal::BigDecimal;
    use num_bigint::BigInt;
    use rstest::*;
    use std::str::FromStr;

    #[rstest]
    #[case::nulls(
        br#"
           null
           null.bool
           null.int
           null.float
           null.decimal
           null.timestamp
           null.symbol
           null.string
           null.clob
           null.blob
           null.list
           null.sexp
           null.struct
        "#,
        vec![
            Null(IonType::Null),
            Null(IonType::Bool),
            Null(IonType::Int),
            Null(IonType::Float),
            Null(IonType::Decimal),
            Null(IonType::Timestamp),
            Null(IonType::Symbol),
            Null(IonType::String),
            Null(IonType::Clob),
            Null(IonType::Blob),
            Null(IonType::List),
            Null(IonType::SExp),
            Null(IonType::Struct),
        ].into_iter().map(|v| v.into()).collect(),
    )]
    #[case::ints(
        br#"
            0
            -65536 65535
            -4294967296 4294967295
            -9007199254740992 9007199254740991
            -18446744073709551616 18446744073709551615
            -79228162514264337593543950336 79228162514264337593543950335
        "#,
        vec![
            0,
            -65536, 65535,
            -4294967296, 4294967295,
            -9007199254740992, 9007199254740991,
        ].into_iter().map(Int::I64).chain(
            vec![
                "-18446744073709551616", "18446744073709551615",
                "-79228162514264337593543950336", "79228162514264337593543950335",
            ].into_iter()
            .map(|v| Int::BigInt(BigInt::parse_bytes(v.as_bytes(), 10).unwrap()))
        ).map(|ai| Int(ai).into()).collect(),
    )]
    #[case::int64_threshold_as_big_int(
        &[0xE0, 0x01, 0x00, 0xEA, 0x28, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        vec![
            "18446744073709551615",
        ].into_iter()
        .map(|v| Int::BigInt(BigInt::parse_bytes(v.as_bytes(), 10).unwrap())).map(|ai| Int(ai).into()).collect(),
    )]
    #[case::int64_threshold_as_int64(
        &[0xE0, 0x01, 0x00, 0xEA, 0x38, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        vec![
            "-9223372036854775808",
        ].into_iter()
        .map(|v| Int::BigInt(BigInt::parse_bytes(v.as_bytes(), 10).unwrap())).map(|ai| Int(ai).into()).collect(),
    )]
    #[case::floats(
        br#"
           1e0 +inf -inf nan
        "#,
        vec![
            1f64, f64::INFINITY, f64::NEG_INFINITY, f64::NAN
        ].into_iter().map(|v| Float(v).into()).collect(),
    )]
    #[case::decimals(
        br#"
            1d0 100d10 -2.1234567d-100
        "#,
        vec![
            "1e0", "100e10", "-2.1234567e-100",
        ].into_iter().map(|s| Decimal(BigDecimal::from_str(s).unwrap().into()).into()).collect(),
    )]
    #[case::timestamps(
        br#"
            2020T
            2020-02-27T
            2020-02-27T14:16:33-00:00
            2020-02-27T14:16:33.123Z
        "#,
        vec![
            TS::with_year(2020).build(),
            TS::with_ymd(2020, 2, 27).build(),
            TS::with_ymd(2020, 2, 27)
                .with_hms(14, 16, 33)
                .build_at_unknown_offset(),
            TS::with_ymd(2020, 2, 27)
                .with_hms(14, 16, 33)
                .with_milliseconds(123)
                .build_at_offset(0),
        ].into_iter().map(|ts_res| Timestamp(ts_res.unwrap()).into()).collect(),
    )]
    #[case::text_symbols(
        br#"
            foo
            'bar'
        "#,
        vec![
            "foo", "bar",
        ].into_iter().map(|s| Symbol(s.into()).into()).collect(),
    )]
    #[case::strings(
        br#"
            '''hello'''
            "world"
        "#,
        vec![
            "hello", "world",
        ].into_iter().map(|s| String(s.into()).into()).collect(),
    )]
    #[case::clobs(
        br#"
            {{'''goodbye'''}}
            {{"moon"}}
        "#,
        {
            // XXX annotate a vector otherwise inference gets a bit confused
            let lobs: Vec<&[u8]> = vec![
                b"goodbye", b"moon",
            ];
            lobs
        }.into_iter().map(|b| Clob(b.into()).into()).collect(),
    )]
    #[case::blobs(
        br#"
           {{bW9v}}
        "#,
        {
            // XXX annotate a vector otherwise inference gets a bit confused
            let lobs: Vec<&[u8]> = vec![
                b"moo",
            ];
            lobs
        }.into_iter().map(|b| Blob(b.into()).into()).collect(),
    )]
    #[case::lists(
        br#"
            ["a", "b"]
        "#,
        vec![
            ion_list!["a", "b"].into()
        ]
    )]
    #[case::sexps(
        br#"
            (e f g)
        "#,
        vec![
            ion_sexp!(Symbol::owned("e") Symbol::owned("f") Symbol::owned("g")).into()
        ]
    )]
    #[case::structs(
        br#"
            {
                bool_field: a::true,
                string_field: a::"moo!",
                string_field: a::"oink!",
            }
        "#,
        vec![
            ion_struct! {
                "string_field": "oink!".with_annotations(["a"]),
                "string_field": "moo!".with_annotations(["a"]),
                "bool_field": true.with_annotations(["a"])
            }.into()
        ]
    )]
    fn read_and_compare(#[case] input: &[u8], #[case] expected: Vec<Element>) -> IonResult<()> {
        let expected: Sequence = expected.into();
        let actual = Element::read_all(input)?;
        assert!(expected.ion_eq(&actual));
        Ok(())
    }
}
