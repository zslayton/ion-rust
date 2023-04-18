use crate::{Decimal, Int, IonType, Timestamp};

// Variants whose corresponding type implements `Copy` do not have a `&`.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueRef<'a, S> {
    Null(IonType),
    Bool(bool),
    Int(Int),
    Float(f64),
    Decimal(Decimal),
    Timestamp(Timestamp),
    String(&'a str),
    Symbol(S),
    Blob(&'a [u8]),
    Clob(&'a [u8]),
    // As ValueRef represents a reference to a value in the streaming APIs, the container variants
    // simply indicate their Ion type. To access their nested data, the reader would need to step in.
    SExp,
    List,
    Struct,
}
