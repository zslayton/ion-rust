use crate::result::{IonError, IonResult};
use crate::serde::tunnel::Tunneled;
use crate::value::owned::{text_token, OwnedElement, OwnedSymbolToken, OwnedValue};
use crate::value::Builder;
use crate::IonType;
use num_bigint::BigInt;
use serde::ser::{
    SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use serde::{Serialize, Serializer};
use std::convert::{TryFrom, TryInto};
use crate::text::writer::TextWriter;
use std::io::Write;
use chrono::DateTime;
use bigdecimal::BigDecimal;

pub(crate) struct WriterSerializer<W: Write> {
    writer: TextWriter<W>
}

impl <W: Write> WriterSerializer<W> {
    pub fn new(writer: TextWriter<W>) -> Self {
        Self {
            writer
        }
    }

    pub fn flush(&mut self) -> IonResult<()> {
        self.writer.flush()
    }
}

impl <W: Write> Serializer for &mut WriterSerializer<W> {
    type Ok = ();
    type Error = IonError;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        self.writer.write_bool(v)?;
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.writer.write_i64(v as i64)?;
        Ok(())
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.writer.write_i64(v as i64)?;
        Ok(())
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.writer.write_i64(v as i64)?;
        Ok(())
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        self.writer.write_i64(v as i64)?;
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.writer.write_i64(v as i64)?;
        Ok(())
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.writer.write_i64(v as i64)?;
        Ok(())
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.writer.write_i64(v as i64)?;
        Ok(())
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        if let Ok(signed) = i64::try_from(v) {
            self.writer.write_i64(signed)?;
        } else {
            todo!("Support writing values larger than i64 can hold.");
        }
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        self.writer.write_f64(v as f64)
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        self.writer.write_f64(v as f64)
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        //TODO: Optimize
        self.writer.write_string(v.to_string())?;
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        self.writer.write_string(v)?;
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        let tunneled = Tunneled::deserialize_ref(v);
        match tunneled {
            Tunneled::Timestamp(t) => {
                todo!("Add a write_timestamp() method to writer");
            },
            Tunneled::Decimal(d) => {
                //TODO: Add a write_decimal() method to writer
                todo!("write_decimal!");
                // let big_decimal: BigDecimal = *d.clone().try_into().unwrap();
                // self.writer.write_decimal(big_decimal)?;
            },
            Tunneled::Blob(b) => {
                self.writer.write_blob(b)?;
            }
            other => todo!("No support for {:?} yet", other),
        };
        Ok(())
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.writer.write_null(IonType::Null)?;
        Ok(())
    }

    fn serialize_some<T: ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.writer.write_null(IonType::Null)?;
        Ok(())
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.writer.write_null(IonType::Null)?;
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        // Write named enum variants as symbols.
        // TODO: Write the enum type as an annotation?
        self.writer.write_symbol(variant)?;
        Ok(())
    }

    fn serialize_newtype_struct<T: ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        todo!()
    }

    fn serialize_newtype_variant<T: ?Sized>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        todo!()
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        // // Create a vector to hold any child values for this sequence.
        // self.sequence_stack.push(Vec::new());
        // Ok(self)
        self.writer.step_in(IonType::List)?;
        Ok(self)
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.writer.step_in(IonType::SExpression)?;
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        todo!()
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        todo!()
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        todo!()
    }

    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.writer.step_in(IonType::Struct)?;
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        todo!()
    }
}

impl <W: Write> WriterSerializer<W> {
    fn serialize_sequence_element<T: ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<(), <&mut WriterSerializer<W> as Serializer>::Error>
    where
        T: Serialize,
    {
        value.serialize(self)
    }
}

impl <W: Write> SerializeSeq for &mut WriterSerializer<W> {
    type Ok = ();
    type Error = IonError;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.serialize_sequence_element(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.writer.step_out()
    }
}

impl <W: Write> SerializeTuple for &mut WriterSerializer<W> {
    type Ok = ();
    type Error = IonError;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.serialize_sequence_element(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.writer.step_out()
    }
}

impl <W: Write> SerializeStruct for &mut WriterSerializer<W> {
    type Ok = ();
    type Error = IonError;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.writer.set_field_name(key);
        let serializer: &mut WriterSerializer<W> = self;
        value.serialize(serializer)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.writer.step_out()
    }
}

impl<W: Write> SerializeMap for &mut WriterSerializer<W> {
    type Ok = ();
    type Error = IonError;

    fn serialize_key<T: ?Sized>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        // Ion cannot support maps with arbitrarily typed keys :(
        todo!()
    }

    fn serialize_value<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        todo!()
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        todo!()
    }
}

//TODO
impl <W: Write> SerializeTupleStruct for &mut WriterSerializer<W> {
    type Ok = ();
    type Error = IonError;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        todo!()
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        todo!()
    }
}

//TODO
impl <W: Write> SerializeTupleVariant for &mut WriterSerializer<W> {
    type Ok = ();
    type Error = IonError;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        todo!()
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        todo!()
    }
}
//TODO
impl <W: Write> SerializeStructVariant for &mut WriterSerializer<W> {
    type Ok = ();
    type Error = IonError;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        todo!()
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde::to_owned_element;
    use crate::types::timestamp::Timestamp;
    use rstest::rstest;
    use serde::Serialize;
    use crate::value::owned::OwnedSequence;
    use std::iter::FromIterator;
    use crate::types::decimal::Decimal;

    fn to_ion_text<T: Serialize>(value: T) -> String {
        let mut buffer: Vec<u8> = vec![];
        let mut writer = TextWriter::new(&mut buffer);
        let mut serializer = WriterSerializer::new(writer);
        value.serialize(&mut serializer).expect("serialize failed");
        serializer.flush().expect("flush failed");
        drop(serializer);
        String::from_utf8(buffer).expect("Not utf8")
    }

    #[rstest]
    #[case::i64_min(i64::MIN)]
    #[case::i32_min(i32::MIN as i64)]
    #[case::i64_min(i16::MIN as i64)]
    #[case::i8_min(i8::MIN as i64)]
    #[case::zero(0i64)]
    #[case::i8_max(i8::MAX as i64)]
    #[case::i16_max(i16::MAX as i64)]
    #[case::i32_max(i32::MAX as i64)]
    #[case::i64_max(i64::MAX)]
    #[case::string_empty(String::from(""))]
    #[case::string_hello_world(String::from("Hello, World!"))]
    #[case::bool_true(true)]
    #[case::bool_false(false)]
    // This test only works with types for which we have an Into<OwnedElement> implementation.
    fn test_serde_serialize_scalars<T: Serialize>(#[case] value: T) {
        println!("{:?}", to_ion_text(value));
    }

    #[test]
    fn test_serde_serialize_struct() {
        #[derive(Serialize)]
        struct Data {
            foo: u32,
            bar: bool,
            baz: Option<u32>,
            quux: String,
            // decimal: Decimal,
        }

        let data = Data {
            foo: 17,
            bar: true,
            baz: None,
            quux: String::from("Hello, world!"),
            // decimal: 100.into()
        };

        println!("{}", to_ion_text(data));
    }


    #[test]
    fn test_serde_serialize_list() {
        let strings = ["foo", "bar", "baz", "quux"];

        println!("{}", to_ion_text(strings));
    }

    #[test]
    fn test_serde_serialize_tuple() {
        let tuple = ("foo", "bar", "baz");

        println!("{}", to_ion_text(tuple));
    }
}
