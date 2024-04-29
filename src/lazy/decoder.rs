use std::fmt::Debug;
use std::ops::Range;

use bumpalo::Bump as BumpAllocator;

use crate::lazy::encoding::RawValueLiteral;
use crate::lazy::expanded::macro_evaluator::RawEExpression;
use crate::lazy::raw_stream_item::LazyRawStreamItem;
use crate::lazy::raw_value_ref::RawValueRef;
use crate::result::IonFailure;
use crate::{IonResult, IonType, RawSymbolTokenRef};

pub trait HasSpan<'top>: HasRange {
    fn span(&self) -> &'top [u8];
}

// impl<'top, T: HasSpan<'top>> HasSpan<'top> for &T {
//     fn span(&self) -> &'top [u8] {
//         (*self).span()
//     }
// }

pub trait HasRange {
    fn range(&self) -> Range<usize>;
}

// impl<T: HasRange> HasRange for &T {
//     fn range(&self) -> Range<usize> {
//         (*self).range()
//     }
// }

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
    /// A symbol token representing the name of a field within a struct.
    type FieldName<'top>: LazyRawFieldName<'top>;
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

impl<V: HasRange, M: HasRange> HasRange for RawValueExpr<V, M> {
    fn range(&self) -> Range<usize> {
        match self {
            RawValueExpr::ValueLiteral(value) => value.range(),
            RawValueExpr::MacroInvocation(eexp) => eexp.range(),
        }
    }
}

impl<'top, V: HasSpan<'top>, M: HasSpan<'top>> HasSpan<'top> for RawValueExpr<V, M> {
    fn span(&self) -> &'top [u8] {
        match self {
            RawValueExpr::ValueLiteral(value) => value.span(),
            RawValueExpr::MacroInvocation(eexp) => eexp.span(),
        }
    }
}

/// A (name, value expression) pair representing a field in a struct.
/// The value expression may be either:
///   * a value literal
///   * an e-expression
#[derive(Copy, Clone, Debug)]
pub struct RawFieldExpr<N: Copy, V: Copy, M: Copy> {
    name: N,
    value_expr: RawValueExpr<V, M>,
}

impl<N: Copy, V: Copy, M: Copy> RawFieldExpr<N, V, M> {
    pub fn new(name: impl Into<N>, value_expr: impl Into<RawValueExpr<V, M>>) -> Self {
        Self {
            name: name.into(),
            value_expr: value_expr.into(),
        }
    }

    pub fn into_pair(self) -> (N, RawValueExpr<V, M>) {
        (self.name, self.value_expr)
    }

    pub fn name(&self) -> N {
        self.name
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
pub type LazyRawFieldExpr<'top, D> = RawFieldExpr<
    <D as LazyDecoder>::FieldName<'top>,
    <D as LazyDecoder>::Value<'top>,
    <D as LazyDecoder>::EExpression<'top>,
>;

impl<N: Copy + Debug, V: Copy + Debug, M: Copy + Debug> RawFieldExpr<N, V, M> {
    pub fn expect_name_value(self) -> IonResult<(N, V)> {
        let RawValueExpr::ValueLiteral(value) = self.value_expr() else {
            return IonResult::decoding_error(format!(
                "expected a name/value pair but found {:?}",
                self
            ));
        };
        Ok((self.into_pair().0, value))
    }

    pub fn expect_name_macro(self) -> IonResult<(N, M)> {
        let RawValueExpr::MacroInvocation(invocation) = self.value_expr() else {
            return IonResult::decoding_error(format!(
                "expected a name/macro pair but found {:?}",
                self
            ));
        };
        Ok((self.into_pair().0, invocation))
    }
}

impl<N: Copy + HasRange, V: Copy + HasRange, M: Copy + HasRange> HasRange
    for RawFieldExpr<N, V, M>
{
    // This type does not offer a `span()` method get get the bytes of the entire field.
    // We could add this in the future, but it comes at the expense of increased data size.
    // The spans for the field name and value can be viewed via their respective accessor methods.
    fn range(&self) -> Range<usize> {
        self.name.range().start..self.value_expr.range().end
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
    use super::{LazyDecoder, LazyRawFieldExpr, LazyRawStruct, LazyRawValueExpr};
    use crate::lazy::expanded::r#struct::UnexpandedField;
    use crate::lazy::expanded::EncodingContext;
    use crate::IonResult;

    pub trait LazyContainerPrivate<'top, D: LazyDecoder> {
        /// Constructs a new lazy raw container from a lazy raw value that has been confirmed to be
        /// of the correct type.
        fn from_value(value: D::Value<'top>) -> Self;
    }

    pub trait LazyRawStructPrivate<'top, D: LazyDecoder> {
        fn unexpanded_fields(
            &self,
            context: EncodingContext<'top>,
        ) -> RawStructUnexpandedFieldsIterator<'top, D>;
    }

    pub struct RawStructUnexpandedFieldsIterator<'top, D: LazyDecoder> {
        context: EncodingContext<'top>,
        raw_fields: <D::Struct<'top> as LazyRawStruct<'top, D>>::Iterator,
    }

    impl<'top, D: LazyDecoder> Iterator for RawStructUnexpandedFieldsIterator<'top, D> {
        type Item = IonResult<UnexpandedField<'top, D>>;

        fn next(&mut self) -> Option<Self::Item> {
            let field: LazyRawFieldExpr<'top, D> = match self.raw_fields.next() {
                Some(Ok(field)) => field,
                Some(Err(e)) => return Some(Err(e)),
                None => return None,
            };
            let unexpanded = match field.value_expr() {
                LazyRawValueExpr::<D>::ValueLiteral(v) => {
                    UnexpandedField::RawNameValue(self.context, field.name(), v)
                }
                LazyRawValueExpr::<D>::MacroInvocation(eexp) => {
                    UnexpandedField::RawNameEExp(self.context, field.name(), eexp)
                }
            };
            Some(Ok(unexpanded))
        }
    }

    impl<'top, D: LazyDecoder<Struct<'top> = S>, S> LazyRawStructPrivate<'top, D> for S
    where
        S: LazyRawStruct<'top, D>,
    {
        fn unexpanded_fields(
            &self,
            context: EncodingContext<'top>,
        ) -> RawStructUnexpandedFieldsIterator<'top, D> {
            let raw_fields = <Self as LazyRawStruct<'top, D>>::iter(self);
            RawStructUnexpandedFieldsIterator {
                context,
                raw_fields,
            }
        }
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
    HasSpan<'top> + RawValueLiteral + Copy + Clone + Debug + Sized
{
    fn ion_type(&self) -> IonType;
    fn is_null(&self) -> bool;
    fn annotations(&self) -> D::AnnotationsIterator<'top>;
    fn read(&self) -> IonResult<RawValueRef<'top, D>>;
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
    private::LazyContainerPrivate<'top, D>
    + private::LazyRawStructPrivate<'top, D>
    + Debug
    + Copy
    + Clone
{
    type Iterator: Iterator<Item = IonResult<LazyRawFieldExpr<'top, D>>>;

    fn annotations(&self) -> D::AnnotationsIterator<'top>;

    fn iter(&self) -> Self::Iterator;
}

pub trait LazyRawFieldName<'top>: HasSpan<'top> + Copy + Debug + Clone {
    fn read(&self) -> IonResult<RawSymbolTokenRef<'top>>;
}
