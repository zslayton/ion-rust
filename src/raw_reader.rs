use crate::types::value_ref::RawValueRef;
use crate::types::IonType;
use crate::{IonResult, RawSymbolTokenRef};
use std::fmt::{Display, Formatter};
use std::io::Read;

pub trait RawIonReader {
    /// Returns the (major, minor) version of the Ion stream being read. If ion_version is called
    /// before an Ion Version Marker has been read, the version (1, 0) will be returned.
    fn ion_version(&self) -> (u8, u8);

    /// Attempts to advance the cursor to the next value in the stream at the current depth.
    /// If no value is encountered, returns None; otherwise, returns the Ion type of the next value.
    fn next(&mut self) -> IonResult<RawStreamItem>;

    /// Returns a value describing the stream entity over which the Reader is currently positioned.
    /// Depending on the Reader's level of abstraction, that entity may or may not be an Ion value.
    fn current(&self) -> RawStreamItem;

    /// If the current item is a value, returns that value's Ion type. Otherwise, returns None.
    fn ion_type(&self) -> Option<IonType>;

    /// Returns an iterator that will yield each of the annotations for the current value in order.
    /// If there is no current value, returns an empty iterator.
    // TODO: Provide a destructive read_annotation() method and a concrete iterator type.
    //       See: https://github.com/amazon-ion/ion-rust/issues/511
    fn annotations<'a>(&'a self)
        -> Box<dyn Iterator<Item = IonResult<RawSymbolTokenRef<'a>>> + 'a>;

    /// If the reader is positioned over a value with one or more annotations, returns `true`.
    /// Otherwise, returns `false`.
    fn has_annotations(&self) -> bool {
        // Implementations are encouraged to override this when there's a cheaper way of
        // determining whether the current value has annotations.
        self.annotations().next().is_some()
    }

    /// Returns the number of annotations on the current value. If there is no current value,
    /// returns zero.
    fn number_of_annotations(&self) -> usize {
        // Implementations are encouraged to override this when there's a cheaper way of
        // calculating the number of annotations.
        self.annotations().count()
    }

    /// If the current item is a field within a struct, returns `Ok(_)` with a [Self::Symbol]
    /// representing the field's name; otherwise, returns an [IonError::IllegalOperation].
    ///
    /// Implementations may also return an error for other reasons; for example, if [Self::Symbol]
    /// is a text data type but the field name is an undefined symbol ID, the reader may return
    /// a decoding error.
    fn field_name(&self) -> IonResult<RawSymbolTokenRef>;

    /// Returns `true` if the reader is currently positioned over an Ion null of any type.
    fn is_null(&self) -> bool;

    fn read_value(&self) -> IonResult<RawValueRef>;

    /// If the current value is a container (i.e. a struct, list, or s-expression), positions the
    /// cursor at the beginning of that container's sequence of child values. The application must
    /// call [Self::next()] to advance to the first child value. If the current value is not a container,
    /// returns [IonError].
    fn step_in(&mut self) -> IonResult<()>;

    /// Positions the cursor at the end of the container currently being traversed. Calling [Self::next()]
    /// will position the cursor over the item that follows the container. If the cursor is not in
    /// a container (i.e. it is already at the top level), returns [IonError].
    fn step_out(&mut self) -> IonResult<()>;

    /// If the reader is positioned at the top level, returns `None`. Otherwise, returns
    /// `Some(_)` with the parent container's [IonType].
    fn parent_type(&self) -> Option<IonType>;

    /// Returns a [usize] indicating the Reader's current level of nesting. That is: the number of
    /// times the Reader has stepped into a container without later stepping out. At the top level,
    /// this method returns `0`.
    fn depth(&self) -> usize;
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
/// Raw stream components that a RawReader may encounter.
pub enum RawStreamItem {
    /// An Ion Version Marker (IVM) indicating the Ion major and minor version that were used to
    /// encode the values that follow.
    VersionMarker(u8, u8),
    /// A non-null Ion value and its corresponding Ion data type.
    /// Stream values that represent system constructs (e.g. a struct marked with a
    /// $ion_symbol_table annotation) are still considered values at the raw level.
    Value(IonType),
    /// A null Ion value and its corresponding Ion data type.
    Null(IonType),
    /// Indicates that the reader is not positioned over anything. This can happen:
    /// * before the reader has begun processing the stream.
    /// * after the reader has stepped into a container, but before the reader has called next()
    /// * after the reader has stepped out of a container, but before the reader has called next()
    /// * after the reader has read the last item in a container
    Nothing,
}

impl RawStreamItem {
    /// If `is_null` is `true`, returns `RawStreamItem::Value(ion_type)`. Otherwise,
    /// returns `RawStreamItem::Null(ion_type)`.
    pub fn nullable_value(ion_type: IonType, is_null: bool) -> RawStreamItem {
        if is_null {
            RawStreamItem::Null(ion_type)
        } else {
            RawStreamItem::Value(ion_type)
        }
    }
}

impl Display for RawStreamItem {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use RawStreamItem::*;
        match self {
            VersionMarker(major, minor) => write!(f, "ion version marker (v{major}.{minor})"),
            Value(ion_type) => write!(f, "{ion_type}"),
            Null(ion_type) => write!(f, "null.{ion_type}"),
            Nothing => write!(f, "nothing/end-of-sequence"),
        }
    }
}

/// BufferedRawReader is a RawReader which can be created from a Vec<u8> and implements the needed
/// functionality to provide non-blocking reader support. This includes the ability to add more
/// data as needed, as well as marking when the stream is complete.
pub trait BufferedRawReader: RawIonReader + From<Vec<u8>> {
    fn append_bytes(&mut self, bytes: &[u8]) -> IonResult<()>;
    fn read_from<R: Read>(&mut self, source: R, length: usize) -> IonResult<usize>;
    // Mark the stream as complete. This allows the reader to understand when partial parses on
    // data boundaries are not possible.
    fn stream_complete(&mut self);
    fn is_stream_complete(&self) -> bool;
}

impl<R: RawIonReader + ?Sized> RawIonReader for Box<R> {
    #[inline]
    fn ion_version(&self) -> (u8, u8) {
        (**self).ion_version()
    }

    fn next(&mut self) -> IonResult<RawStreamItem> {
        (**self).next()
    }

    fn current(&self) -> RawStreamItem {
        (**self).current()
    }

    fn ion_type(&self) -> Option<IonType> {
        (**self).ion_type()
    }

    fn annotations<'a>(&'a self) -> Box<dyn Iterator<Item = IonResult<RawSymbolTokenRef>> + 'a> {
        (**self).annotations()
    }

    fn field_name(&self) -> IonResult<RawSymbolTokenRef> {
        (**self).field_name()
    }

    fn is_null(&self) -> bool {
        (**self).is_null()
    }

    fn read_value(&self) -> IonResult<RawValueRef> {
        (**self).read_value()
    }

    fn step_in(&mut self) -> IonResult<()> {
        (**self).step_in()
    }

    fn step_out(&mut self) -> IonResult<()> {
        (**self).step_out()
    }

    fn parent_type(&self) -> Option<IonType> {
        (**self).parent_type()
    }

    fn depth(&self) -> usize {
        (**self).depth()
    }
}

impl<'a, R: RawIonReader + ?Sized> RawIonReader for &'a mut R {
    #[inline]
    fn ion_version(&self) -> (u8, u8) {
        (**self).ion_version()
    }

    fn next(&mut self) -> IonResult<RawStreamItem> {
        (**self).next()
    }

    fn current(&self) -> RawStreamItem {
        (**self).current()
    }

    fn ion_type(&self) -> Option<IonType> {
        (**self).ion_type()
    }

    fn annotations<'b>(&'b self) -> Box<dyn Iterator<Item = IonResult<RawSymbolTokenRef>> + 'b> {
        (**self).annotations()
    }

    fn field_name(&self) -> IonResult<RawSymbolTokenRef> {
        (**self).field_name()
    }

    fn is_null(&self) -> bool {
        (**self).is_null()
    }

    fn read_value(&self) -> IonResult<RawValueRef> {
        (**self).read_value()
    }

    fn step_in(&mut self) -> IonResult<()> {
        (**self).step_in()
    }

    fn step_out(&mut self) -> IonResult<()> {
        (**self).step_out()
    }

    fn parent_type(&self) -> Option<IonType> {
        (**self).parent_type()
    }

    fn depth(&self) -> usize {
        (**self).depth()
    }
}
