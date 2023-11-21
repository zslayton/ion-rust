use crate::lazy::encoder::binary::{
    BinaryListWriter_1_0, BinarySExpWriter_1_0, BinaryStructWriter_1_0,
};
use crate::lazy::encoder::{AnnotatedValueWriter, ValueWriter};
use crate::lazy::encoding::BinaryEncoding_1_0;
use crate::raw_symbol_token_ref::AsRawSymbolTokenRef;
use crate::{Decimal, Int, IonResult, IonType, RawSymbolTokenRef, SymbolId, Timestamp};
use std::io::Write;
use std::marker::PhantomData;
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

pub struct BinaryValueWriter_1_0<'a> {
    encoding_buffer: &'a mut BumpVec<'a, u8>,
    // allocator: &'a BumpAllocator,
    // The depth at which the value being written will appear. If this value is 0, this is a top-level
    // value writer.
    depth: usize,
    // This implementation doesn't write directly to output, so the generic `W` is unused.
    // However, we need to use it for trait compliance so we store it in a PhantomData.
}

impl<'a> BinaryValueWriter_1_0<'a> {
    pub fn new(
        depth: usize,
        encoding_buffer: &'a mut BumpVec<'a, u8>,
        // allocator: &'c BumpAllocator,
    ) -> Self {
        Self {
            depth,
            encoding_buffer,
            // allocator,
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

    pub fn write_symbol_id(&mut self, symbol_id: SymbolId) -> IonResult<()> {
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

    fn write_lob(&mut self, value: &[u8], type_code: u8) -> IonResult<()> {
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
}

// impl<'a, W: Write> ValueWriter<'a, W, BinaryEncoding_1_0> for BinaryValueWriter_1_0<'a> {
impl<'a> BinaryValueWriter_1_0<'a> {
    fn write_null(&mut self, ion_type: IonType) -> IonResult<()> {
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

    fn write_bool(&mut self, value: bool) -> IonResult<()> {
        let byte: u8 = if value { 0x11 } else { 0x10 };
        self.push_byte(byte);
        Ok(())
    }

    fn write_i64(&mut self, value: i64) -> IonResult<()> {
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

    fn write_int(&mut self, value: &Int) -> IonResult<()> {
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

    fn write_f32(&mut self, value: f32) -> IonResult<()> {
        if value == 0f32 && !value.is_sign_negative() {
            self.push_byte(0x40);
            return Ok(());
        }

        self.push_byte(0x44);
        self.push_bytes(&value.to_be_bytes());
        Ok(())
    }

    fn write_f64(&mut self, value: f64) -> IonResult<()> {
        if value == 0f64 && !value.is_sign_negative() {
            self.push_byte(0x40);
            return Ok(());
        }

        self.push_byte(0x48);
        self.push_bytes(&value.to_be_bytes());
        Ok(())
    }

    fn write_decimal(&mut self, value: &Decimal) -> IonResult<()> {
        let _encoded_size = self.encoding_buffer.encode_decimal_value(value)?;
        Ok(())
    }

    fn write_timestamp(&mut self, value: &Timestamp) -> IonResult<()> {
        let _ = self.encoding_buffer.encode_timestamp_value(value)?;
        Ok(())
    }

    fn write_string<A: AsRef<str>>(&mut self, value: A) -> IonResult<()> {
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

    fn write_symbol<A: AsRawSymbolTokenRef>(&mut self, value: A) -> IonResult<()> {
        match value.as_raw_symbol_token_ref() {
            RawSymbolTokenRef::SymbolId(sid) => self.write_symbol_id(sid),
            RawSymbolTokenRef::Text(_text) => IonResult::illegal_operation(
                "The Ion 1.0 raw binary writer cannot write text symbols.",
            ),
        }
    }

    fn write_clob<A: AsRef<[u8]>>(&mut self, value: A) -> IonResult<()> {
        let bytes: &[u8] = value.as_ref();
        // The clob type descriptor's high nibble is type code 9
        self.write_lob(bytes, 0x90)
    }

    fn write_blob<A: AsRef<[u8]>>(&mut self, value: A) -> IonResult<()> {
        let bytes: &[u8] = value.as_ref();
        // The blob type descriptor's high nibble is type code 10 (0xA)
        self.write_lob(bytes, 0xA0)
    }

    fn list_writer(&mut self) -> IonResult<BinaryListWriter_1_0<'a>> {
        todo!()
    }

    fn sexp_writer(&mut self) -> IonResult<BinarySExpWriter_1_0<'a>> {
        todo!()
    }

    fn struct_writer(&mut self) -> IonResult<BinaryStructWriter_1_0<'a>> {
        todo!()
    }
}

impl<'a, W: Write> ValueWriter<'a, W, BinaryEncoding_1_0> for BinaryValueWriter_1_0<'a> {
    delegate! {
        to (&mut self) {
            fn write_null(mut self, ion_type: IonType) -> IonResult<()>;
            fn write_bool(mut self, value: bool) -> IonResult<()>;
            fn write_i64(mut self, value: i64) -> IonResult<()>;
            fn write_int(mut self, value: &Int) -> IonResult<()>;
            fn write_f32(mut self, value: f32) -> IonResult<()>;
            fn write_f64(mut self, value: f64) -> IonResult<()>;
            fn write_decimal(mut self, value: &Decimal) -> IonResult<()>;
            fn write_timestamp(mut self, value: &Timestamp) -> IonResult<()> ;
            fn write_string<A: AsRef<str>>(mut self, value: A) -> IonResult<()> ;
            fn write_symbol<A: AsRawSymbolTokenRef>(mut self, value: A) -> IonResult<()>;
            fn write_clob<A: AsRef<[u8]>>(mut self, value: A) -> IonResult<()>;
            fn write_blob<A: AsRef<[u8]>>(mut self, value: A) -> IonResult<()>;
            fn list_writer(mut self) -> IonResult<BinaryListWriter_1_0<'a>>;
            fn sexp_writer(mut self) -> IonResult<BinarySExpWriter_1_0<'a>> ;
            fn struct_writer(mut self) -> IonResult<BinaryStructWriter_1_0<'a>>;
        }
    }
}

impl<'a, W: Write> ValueWriter<'a, W, BinaryEncoding_1_0> for &mut BinaryValueWriter_1_0<'a> {
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
            fn list_writer(self) -> IonResult<BinaryListWriter_1_0<'a>>;
            fn sexp_writer(self) -> IonResult<BinarySExpWriter_1_0<'a>> ;
            fn struct_writer(self) -> IonResult<BinaryStructWriter_1_0<'a>>;
        }
    }
}

pub struct BinaryAnnotatedValueWriter_1_0<'a> {
    value_writer: BinaryValueWriter_1_0<'a>,
}

impl<'a> BinaryAnnotatedValueWriter_1_0<'a> {
    pub fn new(value_writer: BinaryValueWriter_1_0<'a>) -> Self {
        Self { value_writer }
    }
}

impl<'a, W: Write> AnnotatedValueWriter<'a, W, BinaryEncoding_1_0>
    for BinaryAnnotatedValueWriter_1_0<'a>
{
    type ValueWriter = BinaryValueWriter_1_0<'a>;

    fn write_annotations<
        SymbolType: AsRawSymbolTokenRef,
        IterType: Iterator<Item = SymbolType> + Clone,
    >(
        self,
        _annotations: IterType,
    ) -> IonResult<BinaryValueWriter_1_0<'a>> {
        todo!("annotations in binary Ion 1.0")
    }

    fn no_annotations(self) -> BinaryValueWriter_1_0<'a> {
        self.value_writer
    }
}

pub struct BinaryTopLevelValueWriter_1_0<'borrow, 'value_writer> {
    value_writer: &'borrow mut BinaryValueWriter_1_0<'value_writer>,
}

impl<'borrow, 'value_writer> BinaryTopLevelValueWriter_1_0<'borrow, 'value_writer> {
    pub fn new(value_writer: &'borrow mut BinaryValueWriter_1_0<'value_writer>) -> Self {
        Self { value_writer }
    }
}

impl<'borrow, 'value_writer, W: Write> AnnotatedValueWriter<'value_writer, W, BinaryEncoding_1_0>
    for BinaryTopLevelValueWriter_1_0<'borrow, 'value_writer>
{
    type ValueWriter = &'borrow mut BinaryValueWriter_1_0<'value_writer>;

    fn write_annotations<
        SymbolType: AsRawSymbolTokenRef,
        IterType: Iterator<Item = SymbolType> + Clone,
    >(
        self,
        annotations: IterType,
    ) -> IonResult<Self::ValueWriter> {
        todo!("annotations in binary Ion 1.0")
    }

    fn no_annotations(self) -> Self::ValueWriter {
        self.value_writer
    }
}

#[cfg(test)]
mod tests {
    use crate::lazy::encoder::binary::LazyRawBinaryWriter_1_0;
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
            //[1, 2, 3]
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
}
