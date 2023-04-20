use std::io;
use std::io::Read;
use std::ops::Range;

use delegate::delegate;

use crate::binary::constants::v1_0::IVM;
use crate::data_source::ToIonDataSource;
use crate::raw_reader::RawIonReader;
use crate::result::{decoding_error, IonResult};
use crate::stream_reader::IonReader;
use crate::symbol_table::SymbolTable;
use crate::value_reader::ValueReader;
use crate::{BlockingRawBinaryReader, BlockingRawTextReader, SystemReader};
use std::fmt::{Display, Formatter};

/// Configures and constructs new instances of [Reader].
pub struct ReaderBuilder {}

impl ReaderBuilder {
    /// Constructs a [ReaderBuilder] pre-populated with common default settings.
    pub fn new() -> ReaderBuilder {
        ReaderBuilder {
            // Eventually, this will contain settings like a `Catalog` implementation.
        }
    }

    /// Applies the specified settings to a new instance of `Reader`. This process involves
    /// reading some data from the beginning of `input` to detect whether its content is
    /// text or binary Ion. If this read operation fails, `build` will return an `Err`
    /// describing the problem it encountered.
    pub fn build<'a, I: 'a + ToIonDataSource>(self, input: I) -> IonResult<Reader<'a>> {
        // Convert the provided input into an implementation of `BufRead`
        let mut input = input.to_ion_data_source();
        // Stack-allocated buffer to hold the first four bytes from input
        let mut header: [u8; 4] = [0u8; 4];

        // Read up to four bytes of input. This has to be done somewhat manually. Convenience
        // functions like `read_exact` will return an error if the input doesn't contain the
        // correct number of bytes, and there are legal Ion streams that have fewer than four
        // bytes in them. (For example, the stream `1 `.)
        let mut total_bytes_read = 0usize;
        while total_bytes_read < IVM.len() {
            let bytes_read = input.read(&mut header[total_bytes_read..])?;
            // If `bytes_read` is zero, we reached the end of the file before we could get
            // all four bytes. That means this isn't a (valid) binary stream. We'll assume
            // it's text.
            if bytes_read == 0 {
                // `header` is a stack-allocated buffer that won't outlive this function call.
                // If it were full, we could move the whole `[u8; 4]` into the reader. However,
                // only some of it is populated and we can't use a slice of it because the array
                // is short-lived. Instead we'll make a statically owned copy of the bytes that
                // we can move into the reader.
                let owned_header = Vec::from(&header[..total_bytes_read]);
                // The file was too short to be binary Ion. Construct a text Reader.
                return Self::make_text_reader(owned_header);
            }
            total_bytes_read += bytes_read;
        }

        // If we've reached this point, we successfully read 4 bytes from the file into `header`.
        // Match against `header` to see if it contains the Ion 1.0 version marker.
        match header {
            [0xe0, 0x01, 0x00, 0xea] => {
                // Binary Ion v1.0
                let full_input = io::Cursor::new(header).chain(input);
                Ok(Self::make_binary_reader(full_input)?)
            }
            [0xe0, major, minor, 0xea] => {
                // Binary Ion v{major}.{minor}
                decoding_error(format!(
                    "cannot read Ion v{major}.{minor}; only v1.0 is supported"
                ))
            }
            _ => {
                // It's not binary, assume it's text
                let full_input = io::Cursor::new(header).chain(input);
                Ok(Self::make_text_reader(full_input)?)
            }
        }
    }

    fn make_text_reader<'a, I: 'a + ToIonDataSource>(data: I) -> IonResult<Reader<'a>> {
        let raw_reader = Box::new(BlockingRawTextReader::new(data)?);
        Ok(Reader::new(raw_reader))
    }

    fn make_binary_reader<'a, I: 'a + ToIonDataSource>(data: I) -> IonResult<Reader<'a>> {
        let raw_reader = Box::new(BlockingRawBinaryReader::new(data)?);
        Ok(Reader::new(raw_reader))
    }
}

impl Default for ReaderBuilder {
    fn default() -> Self {
        ReaderBuilder::new()
    }
}

/// A Reader that uses dynamic dispatch to abstract over the format (text or binary) being
/// read by an underlying [RawIonReader].
pub type Reader<'a> = UserReader<Box<dyn RawIonReader + 'a>>;

/// A streaming Ion reader that resolves symbol IDs into their corresponding text.
///
/// Reader itself is format-agnostic; all format-specific logic is handled by the
/// wrapped [RawIonReader] implementation.
pub struct UserReader<R: RawIonReader> {
    system_reader: SystemReader<R>,
}

impl<R: RawIonReader> UserReader<R> {
    pub(crate) fn new(raw_reader: R) -> UserReader<R> {
        UserReader {
            system_reader: SystemReader::new(raw_reader),
        }
    }
}

// This module exists to allow our integration tests to directly construct a `UserReader`
// with not-yet-supported settings. We want users to use `ReaderBuilder` instead; eventually,
// `ReaderBuilder` will also work for the integration tests and we can remove this.
// See: https://github.com/amazon-ion/ion-rust/issues/484
#[doc(hidden)]
pub mod integration_testing {
    use crate::{RawIonReader, Reader, UserReader};

    pub fn new_reader<'a, R: 'a + RawIonReader>(raw_reader: R) -> Reader<'a> {
        UserReader::new(Box::new(raw_reader))
    }
}

/// Stream components that an application-level [Reader] implementation may encounter.
#[derive(Debug)]
pub enum StreamItem<'r, R: RawIonReader> {
    /// An Ion value and a handle by which to read it
    Value(ValueReader<'r, R>),
    /// Indicates that the reader is not positioned over anything. This can happen:
    /// * before the reader has begun processing the stream.
    /// * after the reader has stepped into a container, but before the reader has called next()
    /// * after the reader has stepped out of a container, but before the reader has called next()
    /// * after the reader has read the last item in a container
    Nothing,
}

impl<'a, R: RawIonReader> Display for StreamItem<'a, R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use StreamItem::*;
        match self {
            Value(value_reader) => write!(f, "{}", value_reader.ion_type()),
            Nothing => Ok(()),
        }
    }
}

impl<R: RawIonReader> UserReader<R> {
    pub fn symbol_table(&self) -> &SymbolTable {
        self.system_reader.symbol_table()
    }

    fn value_reader(&mut self) -> ValueReader<R> {
        let UserReader { system_reader } = self;
        ValueReader::new(system_reader)
    }
}

impl<R: RawIonReader> IonReader for UserReader<R> {
    type Item<'a> = StreamItem<'a, R> where R: 'a;

    fn ion_version(&self) -> (u8, u8) {
        self.system_reader.ion_version()
    }

    /// Advances the raw reader to the next user-level Ion value, processing any system-level directives
    /// encountered along the way.
    // v-- Clippy complains that `next` resembles `Iterator::next()`
    #[allow(clippy::should_implement_trait)]
    fn next(&mut self) -> IonResult<Self::Item<'_>> {
        use crate::SystemStreamItem::*;
        loop {
            match self.system_reader.next()? {
                VersionMarker(_, _) | SymbolTableValue(_) | SymbolTableNull(_) => {
                    // The system reader encountered encoding artifacts like an IVM or
                    // part of a serialized symbol table. The user reader can ignore this
                    // and move on to the next stream item.
                }
                Value(_) | Null(_) => return Ok(StreamItem::Value(self.value_reader())),
                Nothing => return Ok(StreamItem::Nothing),
            }
        }
    }
}

/// Functionality that is only available if the data source we're reading from is in-memory, like
/// a `Vec<u8>` or `&[u8]`.
// TODO: Expose these special case methods through `ValueReader` instead of `UserReader`.
impl<T: AsRef<[u8]>> UserReader<BlockingRawBinaryReader<io::Cursor<T>>> {
    delegate! {
        to self.system_reader {
            pub fn raw_bytes(&self) -> Option<&[u8]>;
            pub fn raw_field_id_bytes(&self) -> Option<&[u8]>;
            pub fn raw_header_bytes(&self) -> Option<&[u8]>;
            pub fn raw_value_bytes(&self) -> Option<&[u8]>;
            pub fn raw_annotations_bytes(&self) -> Option<&[u8]>;

            pub fn field_id_length(&self) -> Option<usize>;
            pub fn field_id_offset(&self) -> Option<usize>;
            pub fn field_id_range(&self) -> Option<Range<usize>>;

            pub fn annotations_length(&self) -> Option<usize>;
            pub fn annotations_offset(&self) -> Option<usize>;
            pub fn annotations_range(&self) -> Option<Range<usize>>;

            pub fn header_length(&self) -> usize;
            pub fn header_offset(&self) -> usize;
            pub fn header_range(&self) -> Range<usize>;

            pub fn value_length(&self) -> usize;
            pub fn value_offset(&self) -> usize;
            pub fn value_range(&self) -> Range<usize>;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::binary::constants::v1_0::IVM;
    use crate::{BlockingRawBinaryReader, SymbolRef, ValueRef};

    use crate::result::IonResult;
    use crate::StreamItem::Value;

    type TestDataSource = io::Cursor<Vec<u8>>;

    // Create a growable byte vector that starts with the Ion 1.0 version marker
    fn ion_data(bytes: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&IVM);
        data.extend_from_slice(bytes);
        data
    }

    // Creates an io::Cursor over the provided data
    fn data_source_for(bytes: &[u8]) -> TestDataSource {
        let data = ion_data(bytes);
        io::Cursor::new(data)
    }

    // Prepends an Ion 1.0 IVM to the provided data and then creates a BinaryIonCursor over it
    fn raw_binary_reader_for(bytes: &[u8]) -> BlockingRawBinaryReader<TestDataSource> {
        use crate::RawStreamItem::*;
        let mut raw_reader =
            BlockingRawBinaryReader::new(data_source_for(bytes)).expect("unable to create reader");
        assert_eq!(raw_reader.ion_type(), None);
        assert_eq!(raw_reader.next(), Ok(VersionMarker(1, 0)));
        assert_eq!(raw_reader.ion_version(), (1u8, 0u8));
        raw_reader
    }

    fn ion_reader_for(bytes: &[u8]) -> Reader {
        ReaderBuilder::new().build(ion_data(bytes)).unwrap()
    }

    const EXAMPLE_STREAM: &[u8] = &[
        // $ion_symbol_table::{imports: $ion_symbol_table, symbols: ["foo", "bar", "baz"]}
        0xEE, // Var len annotations
        0x92, // Annotations + Value length: 21 bytes
        0x81, // Annotations length: 1
        0x83, // Annotation 3 ('$ion_symbol_table')
        0xDE, // Var len struct
        0x8E, // Length: 14 bytes
        0x87, // Field ID 7 ('symbols')
        0xBC, // 12-byte List
        0x83, 0x66, 0x6f, 0x6f, // "foo"
        0x83, 0x62, 0x61, 0x72, // "bar"
        0x83, 0x62, 0x61, 0x7a, // "baz"
        // System: {$10: 1, $11: 2, $12: 3}
        // User: {foo: 1, bar: 2, baz: 3}
        0xD9, // 9-byte struct
        0x8A, // Field ID 10
        0x21, 0x01, // Integer 1
        0x8B, // Field ID 11
        0x21, 0x02, // Integer 2
        0x8C, // Field ID 12
        0x21, 0x03, // Integer 3
    ];

    #[test]
    fn test_read_struct() -> IonResult<()> {
        let mut reader = ion_reader_for(EXAMPLE_STREAM);

        if let Value(mut v) = reader.next()? {
            if let ValueRef::Struct(s) = v.read()? {
                let mut struct_reader = s.reader()?;

                let mut field1 = struct_reader
                    .next_field()?
                    .expect("expected int field 'foo'");
                assert_eq!(field1.read_name()?, SymbolRef::with_text("foo"));
                assert!(matches!(
                    field1.read_value()?,
                    ValueRef::Int(crate::Int::I64(1))
                ));

                let mut field2 = struct_reader
                    .next_field()?
                    .expect("expected int field 'bar'");
                assert_eq!(field2.read_name()?, SymbolRef::with_text("bar"));
                assert!(matches!(
                    field2.read_value()?,
                    ValueRef::Int(crate::Int::I64(2))
                ));

                let mut field3 = struct_reader
                    .next_field()?
                    .expect("expected int field 'baz'");
                assert_eq!(field3.read_name()?, SymbolRef::with_text("baz"));
                assert!(matches!(
                    field3.read_value()?,
                    ValueRef::Int(crate::Int::I64(3))
                ));
            }
        } else {
            panic!("expected a struct");
        }

        Ok(())
    }
}
