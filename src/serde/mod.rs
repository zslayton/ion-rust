mod owned_element_serializer;
pub mod tunnel;
mod writer_serializer;

use std::convert::TryFrom;

use num_bigint::BigInt;
use serde::ser::{
    SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use serde::{Serialize, Serializer};

use crate::result::{illegal_operation_raw, IonError, IonResult};
use crate::serde::owned_element_serializer::OwnedElementSerializer;
use crate::serde::tunnel::Tunneled;
use crate::value::owned::{text_token, OwnedElement, OwnedSymbolToken, OwnedValue};
use crate::value::Builder;
use crate::IonType;
use std::fmt::Display;

impl serde::ser::Error for IonError {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        let text = msg.to_string();
        // TODO: Make a more general 'EncodingError' variant?
        illegal_operation_raw(text)
    }
}

pub fn to_owned_element<S: Serialize>(value: &S) -> IonResult<OwnedElement> {
    let serializer = &mut OwnedElementSerializer::new();
    value.serialize(serializer)
}
