#![allow(non_camel_case_types)]

use crate::lazy::binary::raw::v1_1::immutable_buffer::ImmutableBuffer;
use crate::lazy::binary::raw::v1_1::value::LazyRawBinaryValue_1_1;
use crate::lazy::decoder::{LazyDecoder, LazyRawReader, RawVersionMarker};
use crate::lazy::encoder::private::Sealed;
use crate::lazy::encoding::BinaryEncoding_1_1;
use crate::lazy::raw_stream_item::{EndPosition, LazyRawStreamItem, RawStreamItem};
use crate::result::IonFailure;
use crate::IonResult;

use bumpalo::Bump as BumpAllocator;

pub struct LazyRawBinaryReader_1_1<'data> {
    data: ImmutableBuffer<'data>,
    bytes_to_skip: usize, // Bytes to skip in order to advance to the next item.
}

impl<'data> LazyRawBinaryReader_1_1<'data> {
    fn new(data: &'data [u8]) -> Self {
        Self::new_with_offset(data, 0)
    }

    fn new_with_offset(data: &'data [u8], offset: usize) -> Self {
        let data = ImmutableBuffer::new_with_offset(data, offset);
        Self {
            data,
            bytes_to_skip: 0,
        }
    }

    fn read_ivm<'top>(
        &mut self,
        buffer: ImmutableBuffer<'data>,
    ) -> IonResult<LazyRawStreamItem<'top, BinaryEncoding_1_1>>
    where
        'data: 'top,
    {
        let (marker, _buffer_after_ivm) = buffer.read_ivm()?;
        let (major, minor) = marker.version();
        if (major, minor) != (1, 1) {
            return IonResult::decoding_error(format!(
                "unsupported version of Ion: v{major}.{minor}; only 1.1 is supported by this reader",
            ));
        }
        self.data = buffer;
        self.bytes_to_skip = 4;
        Ok(LazyRawStreamItem::<BinaryEncoding_1_1>::VersionMarker(
            marker,
        ))
    }

    fn read_value<'top>(
        &mut self,
        buffer: ImmutableBuffer<'data>,
    ) -> IonResult<LazyRawStreamItem<'top, BinaryEncoding_1_1>>
    where
        'data: 'top,
    {
        let lazy_value = match ImmutableBuffer::peek_sequence_value(buffer)? {
            Some(lazy_value) => lazy_value,
            None => {
                return Ok(LazyRawStreamItem::<BinaryEncoding_1_1>::EndOfStream(
                    EndPosition::new(self.position()),
                ))
            }
        };
        self.data = buffer;
        self.bytes_to_skip = lazy_value.encoded_value.total_length();
        Ok(RawStreamItem::Value(lazy_value))
    }

    fn advance_to_next_item(&self) -> IonResult<ImmutableBuffer<'data>> {
        if self.data.len() < self.bytes_to_skip {
            return IonResult::incomplete(
                "cannot advance to next item, insufficient data in buffer",
                self.data.offset(),
            );
        }

        if self.bytes_to_skip > 0 {
            Ok(self.data.consume(self.bytes_to_skip))
        } else {
            Ok(self.data)
        }
    }

    pub fn next<'top>(&'top mut self) -> IonResult<LazyRawStreamItem<'top, BinaryEncoding_1_1>>
    where
        'data: 'top,
    {
        let mut buffer = self.advance_to_next_item()?;
        if buffer.is_empty() {
            return Ok(LazyRawStreamItem::<BinaryEncoding_1_1>::EndOfStream(
                EndPosition::new(buffer.offset()),
            ));
        }

        let type_descriptor = buffer.peek_opcode()?;
        if type_descriptor.is_nop() {
            (_, buffer) = buffer.consume_nop_padding(type_descriptor)?;
            if buffer.is_empty() {
                return Ok(LazyRawStreamItem::<BinaryEncoding_1_1>::EndOfStream(
                    EndPosition::new(buffer.offset()),
                ));
            }
        }
        if type_descriptor.is_ivm_start() {
            return self.read_ivm(buffer);
        }
        self.read_value(buffer)
    }

    /// Runs the provided parsing function on this reader's buffer.
    /// If it succeeds, marks the reader  as ready to advance by the 'n' bytes
    /// that were consumed.
    /// If it does not succeed, the `DataSource` remains unchanged.
    pub(crate) fn try_parse_next<
        F: Fn(ImmutableBuffer) -> IonResult<Option<LazyRawBinaryValue_1_1<'data>>>,
    >(
        &mut self,
        parser: F,
    ) -> IonResult<Option<LazyRawBinaryValue_1_1<'data>>> {
        let buffer = self.advance_to_next_item()?;

        let lazy_value = match parser(buffer) {
            Ok(Some(output)) => output,
            Ok(None) => return Ok(None),
            Err(e) => return Err(e),
        };

        // If the value we read doesn't start where we began reading, there was a NOP.
        // let num_nop_bytes = lazy_value.input.offset() - buffer.offset();
        self.bytes_to_skip = lazy_value.encoded_value.total_length();
        Ok(Some(lazy_value))
    }
}

impl<'data> Sealed for LazyRawBinaryReader_1_1<'data> {}

impl<'data> LazyRawReader<'data, BinaryEncoding_1_1> for LazyRawBinaryReader_1_1<'data> {
    fn new(data: &'data [u8]) -> Self {
        Self::new(data)
    }

    fn next<'top>(
        &'top mut self,
        _allocator: &'top BumpAllocator,
    ) -> IonResult<LazyRawStreamItem<'top, BinaryEncoding_1_1>>
    where
        'data: 'top,
    {
        self.next()
    }

    fn resume_at_offset(
        data: &'data [u8],
        offset: usize,
        _saved_state: <BinaryEncoding_1_1 as LazyDecoder>::ReaderSavedState,
    ) -> Self {
        Self::new_with_offset(data, offset)
    }

    fn position(&self) -> usize {
        self.data.offset() + self.bytes_to_skip
    }
}

#[cfg(test)]
mod tests {
    use crate::lazy::binary::raw::v1_1::reader::LazyRawBinaryReader_1_1;
    use crate::{IonResult, IonType};

    #[test]
    fn nop() -> IonResult<()> {
        let data: Vec<u8> = vec![
            0xE0, 0x01, 0x01, 0xEA, // IVM
            0xEC, // 1-Byte NOP
            0xEC, 0xEC, // 2-Byte NOP
            0xEC, 0xEC, 0xEC, // 3-Byte Nop
            0xED, 0x05, 0x00, 0x00, // 4-Byte NOP
            0xEA, // null.null
        ];

        let mut reader = LazyRawBinaryReader_1_1::new(&data);
        let _ivm = reader.next()?.expect_ivm()?;

        assert_eq!(
            reader.next()?.expect_value()?.read()?.expect_null()?,
            IonType::Null
        );

        Ok(())
    }

    #[test]
    fn bools() -> IonResult<()> {
        let data: Vec<u8> = vec![
            0xE0, 0x01, 0x01, 0xEA, // IVM
            0x5E, // true
            0x5F, // false
        ];

        let mut reader = LazyRawBinaryReader_1_1::new(&data);
        let _ivm = reader.next()?.expect_ivm()?;

        assert!(reader.next()?.expect_value()?.read()?.expect_bool()?);

        assert!(!(reader.next()?.expect_value()?.read()?.expect_bool()?));

        Ok(())
    }

    #[test]
    fn integers() -> IonResult<()> {
        use num_bigint::BigInt;

        #[rustfmt::skip]
        let data: Vec<u8> = vec![
            // IVM
            0xE0, 0x01, 0x01, 0xEA,

            // Integer: 0
            0x50,

            // Integer: 17
            0x51, 0x11,

            // Integer: -944
            0x52, 0x50, 0xFC,

            // Integer: 1
            0xF5, 0x03, 0x01,

            // Integer: 147573952589676412929
            0xF5, 0x13, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
        ];

        let mut reader = LazyRawBinaryReader_1_1::new(&data);
        let _ivm = reader.next()?.expect_ivm()?;

        assert_eq!(
            reader.next()?.expect_value()?.read()?.expect_int()?,
            0.into()
        );
        assert_eq!(
            reader.next()?.expect_value()?.read()?.expect_int()?,
            17.into()
        );
        assert_eq!(
            reader.next()?.expect_value()?.read()?.expect_int()?,
            (-944).into()
        );

        assert_eq!(
            reader.next()?.expect_value()?.read()?.expect_int()?,
            1.into()
        );

        assert_eq!(
            reader.next()?.expect_value()?.read()?.expect_int()?,
            BigInt::parse_bytes(b"147573952589676412929", 10)
                .unwrap()
                .into()
        );
        Ok(())
    }

    #[test]
    fn strings() -> IonResult<()> {
        #[rustfmt::skip]
        let data: Vec<u8> = vec![
            // IVM
            0xe0, 0x01, 0x01, 0xea,

            // String: ""
            0x80,

            // String: "hello"
            0x85, 0x68, 0x65, 0x6c, 0x6c, 0x6f,

            // String: "fourteen bytes"
            0x8E, 0x66, 0x6F, 0x75, 0x72, 0x74, 0x65, 0x65, 0x6E, 0x20, 0x62, 0x79, 0x74, 0x65,
            0x73,

            // String: "variable length encoding"
            0xF8, 0x31, 0x76, 0x61, 0x72, 0x69, 0x61, 0x62, 0x6C, 0x65, 0x20, 0x6C, 0x65,
            0x6E, 0x67, 0x74, 0x68, 0x20, 0x65, 0x6E, 0x63, 0x6f, 0x64, 0x69, 0x6E, 0x67,
        ];

        let mut reader = LazyRawBinaryReader_1_1::new(&data);
        let _ivm = reader.next()?.expect_ivm()?;

        assert_eq!(reader.next()?.expect_value()?.read()?.expect_string()?, "");

        assert_eq!(
            reader.next()?.expect_value()?.read()?.expect_string()?,
            "hello"
        );

        assert_eq!(
            reader.next()?.expect_value()?.read()?.expect_string()?,
            "fourteen bytes"
        );

        assert_eq!(
            reader.next()?.expect_value()?.read()?.expect_string()?,
            "variable length encoding"
        );

        Ok(())
    }
}