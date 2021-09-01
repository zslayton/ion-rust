use crate::result::IonError;
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
use std::convert::TryFrom;

pub(crate) struct OwnedElementSerializer {
    // When starting a list or sexp, a new Vec will be pushed onto the sequence_stack.
    // This Vec will store all of the children for the list being serialized.
    // If one of those child values is itself a list or sexp, another Vec will be pushed.
    // Each time the list or sexp is completed, the topmost Vec will be popped off the stack
    // and turned into the OwnedElement representing that sequence.
    sequence_stack: Vec<Vec<OwnedElement>>,
    // Same as `sequence_stack` above, but for (key, value) pairs being built into structs.
    struct_stack: Vec<Vec<(OwnedSymbolToken, OwnedElement)>>,
}

impl OwnedElementSerializer {
    pub fn new() -> Self {
        Self {
            sequence_stack: vec![],
            struct_stack: vec![],
        }
    }
}

impl Serializer for &mut OwnedElementSerializer {
    type Ok = OwnedElement;
    type Error = IonError;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        Ok(v.into())
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        Ok((v as i64).into())
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        Ok((v as i64).into())
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        Ok((v as i64).into())
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        Ok((v as i64).into())
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        Ok((v as i64).into())
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        Ok((v as i64).into())
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        Ok((v as i64).into())
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        if let Ok(signed) = i64::try_from(v) {
            Ok(signed.into())
        } else {
            // The u64 is too large to safely represent as an i64; convert it to a BigInt first.
            let big_int: BigInt = v.into();
            Ok(big_int.into())
        }
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        Ok((v as f64).into())
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        Ok(v.into())
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        // TODO: optimize this after addressing https://github.com/amzn/ion-rust/issues/303
        let element: OwnedElement = OwnedValue::String(v.to_string()).into();
        Ok(element)
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        let element: OwnedElement = OwnedValue::String(v.to_string()).into();
        Ok(element)
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        // let element: OwnedElement = OwnedValue::Blob(v.into()).into();
        // println!("Serializer ptr bytes: {:?}", v);
        let tunneled = Tunneled::deserialize_ref(v);
        match tunneled {
            Tunneled::Timestamp(t) => Ok(OwnedValue::Timestamp((*t).clone()).into()),
            other => todo!("No support for {:?} yet", other),
        }
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Ok(OwnedElement::new_null(IonType::Null))
    }

    fn serialize_some<T: ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(OwnedElement::new_null(IonType::Null))
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok(OwnedElement::new_null(IonType::Null))
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        // Write named enum variants as symbols.
        // TODO: Write the enum type as an annotation?
        Ok(OwnedElement::new_symbol(text_token(variant)))
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
        // Create a vector to hold any child values for this sequence.
        self.sequence_stack.push(Vec::new());
        Ok(self)
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.sequence_stack.push(Vec::new());
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
        self.struct_stack.push(Vec::new());
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

impl OwnedElementSerializer {
    fn serialize_sequence_element<T: ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<(), <&mut OwnedElementSerializer as Serializer>::Error>
    where
        T: Serialize,
    {
        let serializer: &mut OwnedElementSerializer = self;
        let element = match value.serialize(serializer) {
            Ok(element) => element,
            Err(error) => return Err(error),
        };
        self.sequence_stack
            .last_mut()
            .expect("Sequence stack was empty.")
            .push(element);
        Ok(())
    }
}

impl SerializeSeq for &mut OwnedElementSerializer {
    type Ok = OwnedElement;
    type Error = IonError;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.serialize_sequence_element(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let children = self
            .sequence_stack
            .pop()
            .expect("Sequence stack was empty.");
        Ok(OwnedElement::new_list(children))
    }
}

impl SerializeTuple for &mut OwnedElementSerializer {
    type Ok = OwnedElement;
    type Error = IonError;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.serialize_sequence_element(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let children = self
            .sequence_stack
            .pop()
            .expect("Sequence stack was empty.");
        Ok(OwnedElement::new_sexp(children))
    }
}

impl SerializeStruct for &mut OwnedElementSerializer {
    type Ok = OwnedElement;
    type Error = IonError;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        let serializer: &mut OwnedElementSerializer = *self;
        let element = match value.serialize(serializer) {
            Ok(element) => element,
            Err(error) => return Err(error),
        };
        let name_value_pair = (text_token(key), element);
        self.struct_stack
            .last_mut()
            .expect("Sequence stack was empty.")
            .push(name_value_pair);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let children = self.struct_stack.pop().expect("Sequence stack was empty.");
        Ok(OwnedElement::new_struct(children))
    }
}

impl<'a> SerializeMap for &mut OwnedElementSerializer {
    type Ok = OwnedElement;
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
impl SerializeTupleStruct for &mut OwnedElementSerializer {
    type Ok = OwnedElement;
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

impl SerializeTupleVariant for &mut OwnedElementSerializer {
    type Ok = OwnedElement;
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

impl SerializeStructVariant for &mut OwnedElementSerializer {
    type Ok = OwnedElement;
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
    use crate::value::reader::{element_reader, ElementReader};

    fn test_serde_eq_into<T: Serialize + Into<OwnedElement>>(value: T) {
        let serde_element = to_owned_element(&value).expect("serde failed");
        let into_element: OwnedElement = (value).into();
        assert_eq!(
            into_element, serde_element,
            "into() {:?} == serde {:?}",
            into_element, serde_element
        );
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
    fn test_serde_serialize_scalars<T: Serialize + Into<OwnedElement>>(#[case] value: T) {
        test_serde_eq_into(value);
    }

    #[test]
    fn test_serde_serialize_struct() {
        #[derive(Serialize)]
        struct Data {
            foo: u32,
            bar: bool,
            baz: Option<u32>,
            quux: String,
            timestamp: Timestamp, // byte_array: Vec<u8>
        }

        let data = Data {
            foo: 17,
            bar: true,
            baz: None,
            quux: String::from("Hello, world!"),
            timestamp: Timestamp::with_ymd(2021, 9, 7)
                .with_hms(21, 23, 30)
                .with_milliseconds(0)
                .build_at_unknown_offset()
                .unwrap(), // byte_array: vec![0, 1, 2, 3]
        };

        let element = to_owned_element(&data);
        println!("{:?}", element);
    }


    #[test]
    fn test_serde_serialize_list() {
        let strings = ["foo", "bar", "baz", "quux"];

        let data: Vec<_> = strings.iter().collect();
        let actual_list = to_owned_element(&data)
            .expect("Serialization failed.");

        let string_elements: Vec<OwnedElement> = strings
            .iter()
            .map(|s| OwnedValue::String(s.to_string()).into())
            .collect();

        let expected_list: OwnedElement = OwnedValue::List(
            OwnedSequence::from_iter(string_elements)
        ).into();

        assert_eq!(actual_list, expected_list);
        println!("{:?}", actual_list);
    }

    #[test]
    fn test_serde_serialize_tuple() {
        let tuple = ("foo", "bar", "baz");

        let actual_sexp = to_owned_element(&tuple)
            .expect("Serialization failed.");

        let string_elements: Vec<OwnedElement> = vec!["foo", "bar", "baz"]
            .iter()
            .map(|s| OwnedValue::String(s.to_string()).into())
            .collect();

        let expected_sexp: OwnedElement = OwnedValue::SExpression(
            OwnedSequence::from_iter(string_elements)
        ).into();

        let expected_sexp = element_reader()
            .read_one(br#"
                ("foo" "bar" "baz")
            "#)
            .unwrap();

        assert_eq!(actual_sexp, expected_sexp);
        println!("{:?}", actual_sexp);
    }
}
