use crate::lazy::encoder::binary::{
    BinaryContainerWriter_1_0, BinaryListWriter_1_0, BinarySExpWriter_1_0, BinaryStructWriter_1_0,
};
use crate::lazy::encoder::{AnnotatedValueWriter, SequenceWriter, StructWriter, ValueWriter};
use crate::raw_symbol_token_ref::AsRawSymbolTokenRef;
use crate::{Decimal, Int, IonResult, IonType, RawSymbolTokenRef, SymbolId, Timestamp};
use std::mem;

use crate::binary::decimal::DecimalBinaryEncoder;
use crate::binary::timestamp::TimestampBinaryEncoder;
use crate::binary::uint;
use crate::binary::uint::DecodedUInt;
use crate::binary::var_uint::VarUInt;
use crate::result::IonFailure;
use crate::types::integer::IntData;
use bumpalo::collections::Vec as BumpVec;
use bumpalo::Bump as BumpAllocator;
use bytes::BufMut;
use delegate::delegate;
use num_bigint::Sign;
use num_traits::Zero;

// The largest possible 'L' (length) value that can be written directly in a type descriptor byte.
// Larger length values will need to be written as a VarUInt following the type descriptor.
pub(crate) const MAX_INLINE_LENGTH: usize = 13;

pub struct BinaryValueWriter_1_0<'value, 'top> {
    allocator: &'top BumpAllocator,
    encoding_buffer: &'value mut BumpVec<'top, u8>,
}

impl<'value, 'top> BinaryValueWriter_1_0<'value, 'top> {
    pub fn new(
        allocator: &'top BumpAllocator,
        encoding_buffer: &'value mut BumpVec<'top, u8>,
    ) -> Self {
        Self {
            allocator,
            encoding_buffer,
        }
    }

    #[inline]
    fn push_byte(&mut self, byte: u8) {
        self.encoding_buffer.push(byte);
    }

    #[inline]
    fn push_bytes(&mut self, bytes: &[u8]) {
        self.encoding_buffer.extend_from_slice(bytes)
    }

    pub(crate) fn buffer(&self) -> &[u8] {
        self.encoding_buffer.as_slice()
    }

    pub fn write_symbol_id(mut self, symbol_id: SymbolId) -> IonResult<()> {
        const SYMBOL_BUFFER_SIZE: usize = mem::size_of::<u64>();
        let mut buffer = [0u8; SYMBOL_BUFFER_SIZE];
        let mut writer = std::io::Cursor::new(&mut buffer).writer();
        let encoded_length = DecodedUInt::write_u64(&mut writer, symbol_id as u64)?;

        let type_descriptor: u8;
        if encoded_length <= MAX_INLINE_LENGTH {
            type_descriptor = 0x70 | encoded_length as u8;
            self.push_byte(type_descriptor);
        } else {
            type_descriptor = 0x7E;
            self.push_byte(type_descriptor);
            VarUInt::write_u64(self.encoding_buffer, encoded_length as u64)?;
        }
        let raw_buffer = writer.into_inner().into_inner();
        self.push_bytes(&raw_buffer[..encoded_length]);
        Ok(())
    }

    pub fn write_lob(mut self, value: &[u8], type_code: u8) -> IonResult<()> {
        let encoded_length = value.len();
        let type_descriptor: u8;
        if encoded_length <= MAX_INLINE_LENGTH {
            type_descriptor = type_code | encoded_length as u8;
            self.push_byte(type_descriptor);
        } else {
            type_descriptor = type_code | 0x0E;
            self.push_byte(type_descriptor);
            VarUInt::write_u64(self.encoding_buffer, encoded_length as u64)?;
        }
        self.push_bytes(value);
        Ok(())
    }

    pub fn write_null(mut self, ion_type: IonType) -> IonResult<()> {
        let byte: u8 = match ion_type {
            IonType::Null => 0x0F,
            IonType::Bool => 0x1F,
            IonType::Int => 0x2F,
            IonType::Float => 0x4F,
            IonType::Decimal => 0x5F,
            IonType::Timestamp => 0x6F,
            IonType::Symbol => 0x7F,
            IonType::String => 0x8F,
            IonType::Clob => 0x9F,
            IonType::Blob => 0xAF,
            IonType::List => 0xBF,
            IonType::SExp => 0xCF,
            IonType::Struct => 0xDF,
        };
        self.push_byte(byte);
        Ok(())
    }

    pub fn write_bool(mut self, value: bool) -> IonResult<()> {
        let byte: u8 = if value { 0x11 } else { 0x10 };
        self.push_byte(byte);
        Ok(())
    }

    pub fn write_i64(mut self, value: i64) -> IonResult<()> {
        // Get the absolute value of the i64 and store it in a u64.
        let magnitude: u64 = value.unsigned_abs();
        let encoded = uint::encode_u64(magnitude);
        let bytes_to_write = encoded.as_bytes();

        // The encoded length will never be larger than 8 bytes, so it will
        // always fit in the Int's type descriptor byte.
        let encoded_length = bytes_to_write.len();
        let type_descriptor: u8 = if value >= 0 {
            0x20 | (encoded_length as u8)
        } else {
            0x30 | (encoded_length as u8)
        };
        self.push_byte(type_descriptor);
        self.push_bytes(bytes_to_write);

        Ok(())
    }

    pub fn write_int(mut self, value: &Int) -> IonResult<()> {
        // If the `value` is an `i64`, use `write_i64` and return.
        let value = match &value.data {
            IntData::I64(i) => return self.write_i64(*i),
            IntData::BigInt(i) => i,
        };

        // From here on, `value` is a `BigInt`.
        if value.is_zero() {
            self.push_byte(0x20);
            return Ok(());
        }

        let (sign, magnitude_be_bytes) = value.to_bytes_be();

        let mut type_descriptor: u8 = match sign {
            Sign::Plus | Sign::NoSign => 0x20,
            Sign::Minus => 0x30,
        };

        let encoded_length = magnitude_be_bytes.len();
        if encoded_length <= 13 {
            type_descriptor |= encoded_length as u8;
            self.push_byte(type_descriptor);
        } else {
            type_descriptor |= 0xEu8;
            self.push_byte(type_descriptor);
            VarUInt::write_u64(self.encoding_buffer, encoded_length as u64)?;
        }

        self.push_bytes(magnitude_be_bytes.as_slice());

        Ok(())
    }

    pub fn write_f32(mut self, value: f32) -> IonResult<()> {
        if value == 0f32 && !value.is_sign_negative() {
            self.push_byte(0x40);
            return Ok(());
        }

        self.push_byte(0x44);
        self.push_bytes(&value.to_be_bytes());
        Ok(())
    }

    pub fn write_f64(mut self, value: f64) -> IonResult<()> {
        if value == 0f64 && !value.is_sign_negative() {
            self.push_byte(0x40);
            return Ok(());
        }

        self.push_byte(0x48);
        self.push_bytes(&value.to_be_bytes());
        Ok(())
    }

    pub fn write_decimal(self, value: &Decimal) -> IonResult<()> {
        let _encoded_size = self.encoding_buffer.encode_decimal_value(value)?;
        Ok(())
    }

    pub fn write_timestamp(self, value: &Timestamp) -> IonResult<()> {
        let _ = self.encoding_buffer.encode_timestamp_value(value)?;
        Ok(())
    }

    pub fn write_string<A: AsRef<str>>(mut self, value: A) -> IonResult<()> {
        let text: &str = value.as_ref();
        let encoded_length = text.len(); // The number of utf8 bytes

        let type_descriptor: u8;
        if encoded_length <= MAX_INLINE_LENGTH {
            type_descriptor = 0x80 | encoded_length as u8;
            self.push_byte(type_descriptor);
        } else {
            type_descriptor = 0x8E;
            self.push_byte(type_descriptor);
            VarUInt::write_u64(self.encoding_buffer, encoded_length as u64)?;
        }
        self.push_bytes(text.as_bytes());
        Ok(())
    }

    pub fn write_symbol<A: AsRawSymbolTokenRef>(self, value: A) -> IonResult<()> {
        match value.as_raw_symbol_token_ref() {
            RawSymbolTokenRef::SymbolId(sid) => self.write_symbol_id(sid),
            RawSymbolTokenRef::Text(_text) => IonResult::illegal_operation(
                "The Ion 1.0 raw binary writer cannot write text symbols.",
            ),
        }
    }

    pub fn write_clob<A: AsRef<[u8]>>(self, value: A) -> IonResult<()> {
        let bytes: &[u8] = value.as_ref();
        // The clob type descriptor's high nibble is type code 9
        self.write_lob(bytes, 0x90)
    }

    pub fn write_blob<A: AsRef<[u8]>>(self, value: A) -> IonResult<()> {
        let bytes: &[u8] = value.as_ref();
        // The blob type descriptor's high nibble is type code 10 (0xA)
        self.write_lob(bytes, 0xA0)
    }

    fn list_writer(self) -> IonResult<BinaryListWriter_1_0<'value, 'top>> {
        Ok(BinaryListWriter_1_0::new(BinaryContainerWriter_1_0::new(
            0xB0,
            self.allocator,
            self.encoding_buffer,
        )))
    }

    fn sexp_writer(self) -> IonResult<BinarySExpWriter_1_0<'value, 'top>> {
        Ok(BinarySExpWriter_1_0::new(BinaryContainerWriter_1_0::new(
            0xC0,
            self.allocator,
            self.encoding_buffer,
        )))
    }

    fn struct_writer(self) -> IonResult<BinaryStructWriter_1_0<'value, 'top>> {
        let container_writer =
            BinaryContainerWriter_1_0::new(0xD0, &self.allocator, self.encoding_buffer);
        Ok(BinaryStructWriter_1_0::new(container_writer))
    }

    fn write_list<
        F: for<'a> FnMut(
            &'a mut BinaryListWriter_1_0<'value, 'top>,
        ) -> IonResult<&'a mut BinaryListWriter_1_0<'value, 'top>>,
    >(
        self,
        mut list_fn: F,
    ) -> IonResult<()> {
        let mut list_writer = self.list_writer()?;
        list_fn(&mut list_writer)?;
        list_writer.end()
    }

    fn write_sexp<
        F: for<'a> FnMut(
            &'a mut BinarySExpWriter_1_0<'value, 'top>,
        ) -> IonResult<&'a mut BinarySExpWriter_1_0<'value, 'top>>,
    >(
        self,
        mut sexp_fn: F,
    ) -> IonResult<()> {
        let mut sexp_writer = self.sexp_writer()?;
        sexp_fn(&mut sexp_writer)?;
        sexp_writer.end()
    }

    fn write_struct<
        F: for<'a> FnMut(
            &'a mut BinaryStructWriter_1_0<'value, 'top>,
        ) -> IonResult<&'a mut BinaryStructWriter_1_0<'value, 'top>>,
    >(
        self,
        mut struct_fn: F,
    ) -> IonResult<()> {
        let mut struct_writer = self.struct_writer()?;
        struct_fn(&mut struct_writer)?;
        struct_writer.end()
    }
}

impl<'value, 'top> ValueWriter for BinaryValueWriter_1_0<'value, 'top> {
    type ListWriter = BinaryListWriter_1_0<'value, 'top>;
    type SExpWriter = BinarySExpWriter_1_0<'value, 'top>;
    type StructWriter = BinaryStructWriter_1_0<'value, 'top>;

    delegate! {
        to self {
            fn write_null(self, ion_type: IonType) -> IonResult<()>;
            fn write_bool(self, value: bool) -> IonResult<()>;
            fn write_i64(self, value: i64) -> IonResult<()>;
            fn write_int(self, value: &Int) -> IonResult<()>;
            fn write_f32(self, value: f32) -> IonResult<()>;
            fn write_f64(self, value: f64) -> IonResult<()>;
            fn write_decimal(self, value: &Decimal) -> IonResult<()>;
            fn write_timestamp(self, value: &Timestamp) -> IonResult<()> ;
            fn write_string<A: AsRef<str>>(self, value: A) -> IonResult<()> ;
            fn write_symbol<A: AsRawSymbolTokenRef>(self, value: A) -> IonResult<()>;
            fn write_clob<A: AsRef<[u8]>>(self, value: A) -> IonResult<()>;
            fn write_blob<A: AsRef<[u8]>>(self, value: A) -> IonResult<()>;
            fn write_list<F: FnMut(&mut Self::ListWriter) -> IonResult<&mut Self::ListWriter>>(
                self,
                list_fn: F,
            ) -> IonResult<()>;
            fn write_sexp<F: FnMut(&mut Self::SExpWriter) -> IonResult<&mut Self::SExpWriter>>(
                self,
                sexp_fn: F,
            ) -> IonResult<()>;
            fn write_struct<F: FnMut(&mut Self::StructWriter) -> IonResult<&mut Self::StructWriter>>(
                self,
                struct_fn: F,
            ) -> IonResult<()>;
        }
    }
}

pub struct BinaryAnnotatedValueWriter_1_0<'value, 'top> {
    value_writer: BinaryValueWriter_1_0<'value, 'top>,
}

impl<'value, 'top> BinaryAnnotatedValueWriter_1_0<'value, 'top> {
    pub fn new(value_writer: BinaryValueWriter_1_0<'value, 'top>) -> Self {
        Self { value_writer }
    }
}

impl<'value, 'top> AnnotatedValueWriter for BinaryAnnotatedValueWriter_1_0<'value, 'top> {
    type ValueWriter = BinaryValueWriter_1_0<'value, 'top>;

    fn with_annotations<
        SymbolType: AsRawSymbolTokenRef,
        IterType: Iterator<Item = SymbolType> + Clone,
    >(
        self,
        _annotations: IterType,
    ) -> IonResult<BinaryValueWriter_1_0<'value, 'top>> {
        todo!("annotations in binary Ion 1.0")
    }

    fn without_annotations(self) -> BinaryValueWriter_1_0<'value, 'top> {
        self.value_writer
    }
}

#[cfg(test)]
mod tests {
    use crate::lazy::encoder::binary::LazyRawBinaryWriter_1_0;
    use crate::lazy::encoder::{AnnotatedValueWriter, LazyRawWriter, SequenceWriter, StructWriter};
    use crate::{Element, IonData, IonResult, RawSymbolTokenRef, Timestamp};

    fn writer_test(
        expected: &str,
        mut test: impl FnMut(&mut LazyRawBinaryWriter_1_0<&mut Vec<u8>>) -> IonResult<()>,
    ) -> IonResult<()> {
        let expected = Element::read_all(expected)?;
        let mut buffer = Vec::new();
        let mut writer = LazyRawBinaryWriter_1_0::new(&mut buffer)?;
        test(&mut writer)?;
        writer.flush()?;
        let actual = Element::read_all(buffer)?;
        assert!(
            IonData::eq(&expected, &actual),
            "Actual {actual:?} was not equal to {expected:?}"
        );
        Ok(())
    }

    #[test]
    fn write_scalars() -> IonResult<()> {
        let expected = r#"
            1
            false
            3e0
            "foo"
            name
            2023-11-09T
            {{4AEA6g==}}
        "#;
        let test = |writer: &mut LazyRawBinaryWriter_1_0<&mut Vec<u8>>| {
            writer
                .write(1)?
                .write(false)?
                .write(3f32)?
                .write("foo")?
                .write(RawSymbolTokenRef::SymbolId(4))?
                .write(Timestamp::with_ymd(2023, 11, 9).build()?)?
                .write([0xE0u8, 0x01, 0x00, 0xEA])?;
            // .write([1, 2, 3])?;
            Ok(())
        };
        writer_test(expected, test)
    }

    #[test]
    fn write_empty_list() -> IonResult<()> {
        let expected = "[]";
        let test = |writer: &mut LazyRawBinaryWriter_1_0<&mut Vec<u8>>| {
            writer.value_writer().write_list(|list| Ok(list))
        };
        writer_test(expected, test)
    }

    #[test]
    fn write_list() -> IonResult<()> {
        let expected = r#"
            [
                1,
                false,
                3e0,
                "foo",
                name,
                2023-11-09T,
                {{4AEA6g==}},
                // Nested list
                [1, 2, 3],
            ]
        "#;
        let test = |writer: &mut LazyRawBinaryWriter_1_0<&mut Vec<u8>>| {
            writer.value_writer().write_list(|list| {
                list.write(1)?
                    .write(false)?
                    .write(3f32)?
                    .write("foo")?
                    .write(RawSymbolTokenRef::SymbolId(4))?
                    .write(Timestamp::with_ymd(2023, 11, 9).build()?)?
                    .write([0xE0u8, 0x01, 0x00, 0xEA])?
                    .write([1, 2, 3])
            })
        };
        writer_test(expected, test)
    }

    #[test]
    fn write_empty_sexp() -> IonResult<()> {
        let expected = "()";
        let test = |writer: &mut LazyRawBinaryWriter_1_0<&mut Vec<u8>>| {
            writer.value_writer().write_sexp(|sexp| Ok(sexp))
        };
        writer_test(expected, test)
    }

    #[test]
    fn write_sexp() -> IonResult<()> {
        let expected = r#"
            (
                1
                false
                3e0
                "foo"
                name
                2023-11-09T
                {{4AEA6g==}}
                // Nested list
                [1, 2, 3]
            )
        "#;
        let test = |writer: &mut LazyRawBinaryWriter_1_0<&mut Vec<u8>>| {
            writer.value_writer().write_sexp(|sexp| {
                sexp.write(1)?
                    .write(false)?
                    .write(3f32)?
                    .write("foo")?
                    .write(RawSymbolTokenRef::SymbolId(4))?
                    .write(Timestamp::with_ymd(2023, 11, 9).build()?)?
                    .write([0xE0u8, 0x01, 0x00, 0xEA])?
                    .write([1, 2, 3])
            })
        };
        writer_test(expected, test)
    }

    #[test]
    fn write_empty_struct() -> IonResult<()> {
        let expected = "{}";
        let test = |writer: &mut LazyRawBinaryWriter_1_0<&mut Vec<u8>>| {
            writer.value_writer().write_struct(|struct_| Ok(struct_))
        };
        writer_test(expected, test)
    }

    #[test]
    fn write_struct() -> IonResult<()> {
        let expected = r#"
            // This test uses symbol ID field names because the raw writer has no symbol table. 
            {
                $0: 1,
                $1: false,
                $2: 3e0,
                $3: "foo",
                $4: name,
                $5: 2023-11-09T,
                $6: {{4AEA6g==}},
                // Nested list
                $7: [1, 2, 3],
            }
        "#;
        let test = |writer: &mut LazyRawBinaryWriter_1_0<&mut Vec<u8>>| {
            writer.value_writer().write_struct(|struct_| {
                struct_
                    .write(0, 1)?
                    .write(1, false)?
                    .write(2, 3f32)?
                    .write(3, "foo")?
                    .write(4, RawSymbolTokenRef::SymbolId(4))?
                    .write(5, Timestamp::with_ymd(2023, 11, 9).build()?)?
                    .write(6, [0xE0u8, 0x01, 0x00, 0xEA])?
                    .write(7, [1, 2, 3])
            })
        };
        writer_test(expected, test)
    }
}
