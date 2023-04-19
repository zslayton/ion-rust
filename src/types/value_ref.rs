use crate::value_reader::{SequenceRef, StructRef};
use crate::{Decimal, Int, IonType, RawIonReader, RawSymbolTokenRef, Symbol, Timestamp};

// As RawValueRef represents a reference to a value in the streaming APIs, the container variants
// simply indicate their Ion type. To access their nested data, the reader would need to step in.
#[derive(Debug, PartialEq)]
pub enum RawValueRef<'a> {
    Null(IonType),
    Bool(bool),
    Int(Int),
    Float(f64),
    Decimal(Decimal),
    Timestamp(Timestamp),
    String(&'a str),
    Symbol(RawSymbolTokenRef<'a>),
    Blob(&'a [u8]),
    Clob(&'a [u8]),
    // As ValueRef represents a reference to a value in the streaming APIs, the container variants
    // simply indicate their Ion type. To access their nested data, the reader would need to step in.
    SExp,
    List,
    Struct,
}

#[derive(Debug)]
pub enum ValueRef<'a, R: RawIonReader> {
    Null(IonType),
    Bool(bool),
    Int(Int),
    Float(f64),
    Decimal(Decimal),
    Timestamp(Timestamp),
    String(&'a str),
    Symbol(Symbol),
    Blob(&'a [u8]),
    Clob(&'a [u8]),
    // As ValueRef represents a reference to a value in the streaming APIs, the container variants
    // simply indicate their Ion type. To access their nested data, the reader would need to step in.
    SExp(SequenceRef<'a, R>),
    List(SequenceRef<'a, R>),
    Struct(StructRef<'a, R>),
}
