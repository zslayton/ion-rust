use std::fmt::Debug;
use std::ops::Range;

use bumpalo::Bump as BumpAllocator;

use crate::lazy::expanded::macro_evaluator::RawEExpression;
use crate::lazy::raw_stream_item::LazyRawStreamItem;
use crate::lazy::raw_value_ref::RawValueRef;
use crate::result::IonFailure;
use crate::{IonResult, IonType, RawSymbolTokenRef};

/// A family of types that collectively comprise the lazy reader API for an Ion serialization
/// format. These types operate at the 'raw' level; they do not attempt to resolve symbols
/// using the active symbol table.
// Implementations of this trait are typically unit structs that are never instantiated.
// However, many types are generic over some `D: LazyDecoder`, and having this trait
// extend 'static, Sized, Debug, Clone and Copy means that those types can #[derive(...)]
// those traits themselves without boilerplate `where` clauses.
pub trait LazyDecoder: 'static + Sized + Debug + Clone + Copy {
    /// A lazy reader that yields [`Self::Value`]s representing the top level values in its input.
    type Reader<'data>: LazyRawReader<'data, Self>;
    /// Additional data (beyond the offset) that the reader will need in order to resume reading
    /// from a different point in the stream.
    // At the moment this feature is only used by `LazyAnyRawReader`, which needs to remember what
    // encoding the stream was using during earlier read operations.
    type ReaderSavedState: Copy + Default;
    /// A value (at any depth) in the input. This can be further inspected to access either its
    /// scalar data or, if it is a container, to view it as [`Self::List`], [`Self::SExp`] or
    /// [`Self::Struct`].  
    type Value<'top>: LazyRawValue<'top, Self>;
    /// A list whose child values may be accessed iteratively.
    type SExp<'top>: LazyRawSequence<'top, Self>;
    /// An s-expression whose child values may be accessed iteratively.
    type List<'top>: LazyRawSequence<'top, Self>;
    /// A struct whose fields may be accessed iteratively or by field name.
    type Struct<'top>: LazyRawStruct<'top, Self>;
    /// An iterator over the annotations on the input stream's values.
    type AnnotationsIterator<'top>: Iterator<Item = IonResult<RawSymbolTokenRef<'top>>>;
    /// An e-expression invoking a macro. (Ion 1.1+)
    type EExpression<'top>: RawEExpression<'top, Self>;
}

/// An expression found in value position in either serialized Ion or a template.
/// If it is a value literal, it is considered a stream with exactly one Ion value.
/// If it is a macro invocation, it is a stream with zero or more Ion values.
///
/// When working with `RawValueExpr`s that always use a given decoder's `Value` and
/// `MacroInvocation` associated types, consider using [`LazyRawValueExpr`] instead.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RawValueExpr<V, M> {
    /// A value literal. For example: `5`, `foo`, or `"hello"` in text.
    ValueLiteral(V),
    /// An Ion 1.1+ macro invocation. For example: `(:employee 12345 "Sarah" "Gonzalez")` in text.
    MacroInvocation(M),
}

// `RawValueExpr` above has no ties to a particular encoding. The `LazyRawValueExpr` type alias
// below uses the `Value` and `MacroInvocation` associated types from the decoder `D`. In most
// places, this is a helpful constraint; we can talk about the value expression in terms of the
// LazyDecoder it's associated with. However, in some places (primarily when expanding template
// values that don't have a LazyDecoder) we need to be able to use it without constraints.

/// An item found in value position within an Ion data stream written in the encoding represented
/// by the LazyDecoder `D`. This item may be either a value literal or a macro invocation.
///
/// For a version of this type that is not constrained to a particular encoding, see
/// [`RawValueExpr`].
pub type LazyRawValueExpr<'top, D> =
    RawValueExpr<<D as LazyDecoder>::Value<'top>, <D as LazyDecoder>::EExpression<'top>>;

impl<V: Debug, M: Debug> RawValueExpr<V, M> {
    pub fn expect_value(self) -> IonResult<V> {
        match self {
            RawValueExpr::ValueLiteral(v) => Ok(v),
            RawValueExpr::MacroInvocation(_m) => IonResult::decoding_error(
                "expected a value literal, but found a macro invocation ({:?})",
            ),
        }
    }

    pub fn expect_macro(self) -> IonResult<M> {
        match self {
            RawValueExpr::ValueLiteral(v) => IonResult::decoding_error(format!(
                "expected a macro invocation but found a value literal ({:?})",
                v
            )),
            RawValueExpr::MacroInvocation(m) => Ok(m),
        }
    }
}

/// An item found in field position within a struct.
/// This item may be:
///   * a name/value pair (as it is in Ion 1.0)
///   * a name/e-expression pair
///   * an e-expression
#[derive(Clone, Debug)]
pub struct RawFieldExpr<'top, V: Copy, M: Copy> {
    name: RawSymbolTokenRef<'top>,
    value_expr: RawValueExpr<V, M>,
}

impl<'top, V: Copy, M: Copy> RawFieldExpr<'top, V, M> {
    pub fn new(name: RawSymbolTokenRef<'top>, value_expr: impl Into<RawValueExpr<V, M>>) -> Self {
        Self {
            name,
            value_expr: value_expr.into(),
        }
    }

    pub fn into_name_value(self) -> (RawSymbolTokenRef<'top>, RawValueExpr<V, M>) {
        (self.name, self.value_expr)
    }

    pub fn name(&self) -> &RawSymbolTokenRef<'top> {
        &self.name
    }
    pub fn value_expr(&self) -> RawValueExpr<V, M> {
        self.value_expr
    }
}

// As with the `RawValueExpr`/`LazyRawValueExpr` type pair, a `RawFieldExpr` has no constraints
// on the types used for values or macros, while the `LazyRawFieldExpr` type alias below uses the
// value and macro types associated with the decoder `D`.

/// An item found in struct field position an Ion data stream written in the encoding represented
/// by the LazyDecoder `D`.
pub type LazyRawFieldExpr<'top, D> =
    RawFieldExpr<'top, <D as LazyDecoder>::Value<'top>, <D as LazyDecoder>::EExpression<'top>>;

impl<'name, V: Copy + Debug, M: Copy + Debug> RawFieldExpr<'name, V, M> {
    pub fn expect_name_value(self) -> IonResult<(RawSymbolTokenRef<'name>, V)> {
        let RawValueExpr::ValueLiteral(value) = self.value_expr() else {
            return IonResult::decoding_error(format!(
                "expected a name/value pair but found {:?}",
                self
            ));
        };
        Ok((self.into_name_value().0, value))
    }

    pub fn expect_name_macro(self) -> IonResult<(RawSymbolTokenRef<'name>, M)> {
        let RawValueExpr::MacroInvocation(invocation) = self.value_expr() else {
            return IonResult::decoding_error(format!(
                "expected a name/macro pair but found {:?}",
                self
            ));
        };
        Ok((self.into_name_value().0, invocation))
    }
}

// This private module houses public traits. This allows the public traits below to depend on them,
// but keeps users from being able to use them.
//
// For example: `LazyRawField` is a public trait that extends `LazyRawFieldPrivate`, a trait that
// contains functions which are implementation details we reserve the right to change at any time.
// `LazyRawFieldPrivate` is a public trait that lives in a crate-visible module. This allows
// internal code that is defined in terms of `LazyRawField` to call the private `into_value()`
// function while also preventing users from seeing or depending on it.
pub(crate) mod private {
    use crate::lazy::encoding::RawValueLiteral;
    use crate::{IonResult, RawSymbolTokenRef};

    use super::LazyDecoder;

    pub trait LazyRawFieldPrivate<'top, D: LazyDecoder> {
        /// Converts the `LazyRawField` impl to a `LazyRawValue` impl.
        // At the moment, `LazyRawField`s are just thin wrappers around a `LazyRawValue` that can
        // safely assume that the value has a field name associated with it. This method allows
        // us to convert from one to the other when needed.
        fn into_value(self) -> D::Value<'top>;

        /// Returns the input data from which this field was parsed.
        fn input_span(&self) -> &[u8];

        /// Returns the offset at which the input data containing this field began.
        fn input_offset(&self) -> usize;
    }

    pub trait LazyContainerPrivate<'top, D: LazyDecoder> {
        /// Constructs a new lazy raw container from a lazy raw value that has been confirmed to be
        /// of the correct type.
        fn from_value(value: D::Value<'top>) -> Self;
    }

    pub trait LazyRawValuePrivate<'top>: RawValueLiteral {
        /// Returns the field name associated with this value. If the value is not inside a struct,
        /// returns `IllegalOperation`.
        fn field_name(&self) -> IonResult<RawSymbolTokenRef<'top>>;
    }
}

pub trait LazyRawReader<'data, D: LazyDecoder>: Sized {
    fn new(data: &'data [u8]) -> Self {
        Self::resume_at_offset(data, 0, D::ReaderSavedState::default())
    }

    fn resume_at_offset(data: &'data [u8], offset: usize, saved_state: D::ReaderSavedState)
        -> Self;
    fn next<'top>(
        &'top mut self,
        allocator: &'top BumpAllocator,
    ) -> IonResult<LazyRawStreamItem<'top, D>>
    where
        'data: 'top;

    fn save_state(&self) -> D::ReaderSavedState {
        D::ReaderSavedState::default()
    }

    /// The stream byte offset at which the reader will begin parsing the next item to return.
    /// This position is not necessarily the first byte of the next value; it may be (e.g.) a NOP,
    /// a comment, or whitespace that the reader will traverse as part of matching the next item.
    fn position(&self) -> usize;
}

pub trait LazyRawValue<'top, D: LazyDecoder>:
    private::LazyRawValuePrivate<'top> + Copy + Clone + Debug + Sized
{
    fn ion_type(&self) -> IonType;
    fn is_null(&self) -> bool;
    fn annotations(&self) -> D::AnnotationsIterator<'top>;
    fn read(&self) -> IonResult<RawValueRef<'top, D>>;

    fn range(&self) -> Range<usize>;
    fn span(&self) -> &[u8];
}

pub trait LazyRawSequence<'top, D: LazyDecoder>:
    private::LazyContainerPrivate<'top, D> + Debug + Copy + Clone
{
    type Iterator: Iterator<Item = IonResult<LazyRawValueExpr<'top, D>>>;
    fn annotations(&self) -> D::AnnotationsIterator<'top>;
    fn ion_type(&self) -> IonType;
    fn iter(&self) -> Self::Iterator;
    fn as_value(&self) -> D::Value<'top>;
}

pub trait LazyRawStruct<'top, D: LazyDecoder>:
    private::LazyContainerPrivate<'top, D> + Debug + Copy + Clone
{
    type Iterator: Iterator<Item = IonResult<LazyRawFieldExpr<'top, D>>>;

    fn annotations(&self) -> D::AnnotationsIterator<'top>;

    fn iter(&self) -> Self::Iterator;
}

pub trait LazyRawField<'top, D: LazyDecoder>:
    private::LazyRawFieldPrivate<'top, D> + Debug
{
    fn name(&self) -> RawSymbolTokenRef<'top>;
    fn value(&self) -> D::Value<'top>;

    /// Returns the stream offset range that contains the encoded bytes of both the field
    /// name and the field value.
    ///
    /// If there are additional bytes between the field name and value, they will also be
    /// part of the range. In text, this includes the delimiting `:`, whitespace, and potentially
    /// comments.
    fn range(&self) -> Range<usize> {
        let name_start = self.name_range().start;
        let value_end = self.value_range().end;
        name_start..value_end
    }

    /// Returns the input span that contains the encoded bytes of both the field name and the
    /// field value.
    ///
    /// If there are additional bytes between the field name and value, they will also be
    /// part of the range. In text, this includes the delimiting `:`, whitespace, and potentially
    /// comments.
    fn span(&self) -> &[u8] {
        let stream_range = self.range();
        let input = self.input_span();
        let offset = self.input_offset();
        let local_range = (stream_range.start - offset)..(stream_range.end - offset);
        input
            .get(local_range)
            .expect("field name bytes not in buffer")
    }

    /// Returns the stream offset range that contains the encoded bytes of the field's name.
    fn name_range(&self) -> Range<usize>;
    /// Returns the input span that contains the encoded bytes of the field's name.
    fn name_span(&self) -> &[u8];

    /// Returns the stream offset range that contains the encoded bytes of the field's value.
    fn value_range(&self) -> Range<usize> {
        self.value().range()
    }
    /// Returns the input span that contains the encoded bytes of the field's value.
    fn value_span(&self) -> &[u8] {
        let stream_range = self.value_range();
        let input = self.input_span();
        let offset = self.input_offset();
        let local_range = (stream_range.start - offset)..(stream_range.end - offset);
        input
            .get(local_range)
            .expect("field value bytes not in buffer")
    }
}
