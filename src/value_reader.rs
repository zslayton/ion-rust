use crate::{
    IonResult, IonType, RawIonReader, SymbolRef, SystemReader, SystemStreamItem, ValueRef,
};
use std::fmt::{Debug, Formatter};

pub struct StructRef<'r, R: RawIonReader> {
    reader: &'r mut SystemReader<R>,
}

impl<'r, R: RawIonReader> Debug for StructRef<'r, R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "StructRef")
    }
}

impl<'r, R: RawIonReader> StructRef<'r, R> {
    pub(crate) fn new(reader: &mut SystemReader<R>) -> StructRef<R> {
        StructRef { reader }
    }

    pub fn reader(self) -> IonResult<StructReader<'r, R>> {
        let StructRef { reader } = self;
        reader.step_in()?;
        Ok(StructReader::new(reader))
    }
}

pub struct SequenceRef<'r, R: RawIonReader> {
    reader: &'r mut SystemReader<R>,
}

impl<'r, R: RawIonReader> SequenceRef<'r, R> {
    pub(crate) fn new(reader: &mut SystemReader<R>) -> SequenceRef<R> {
        SequenceRef { reader }
    }

    pub fn reader(self) -> IonResult<SequenceReader<'r, R>> {
        let SequenceRef { reader } = self;
        reader.step_in()?;
        Ok(SequenceReader::new(reader))
    }
}

impl<'r, R: RawIonReader> Debug for SequenceRef<'r, R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "SequenceRef")
    }
}

/// A `ValueReader` holds a reference to the current value in the data stream. It does not interpret
/// or materialize the value unless requested to do so by the user.
///
/// **Reading scalar values**
/// ```
/// use ion_rs::{IonReader, IonResult, ReaderBuilder, ValueRef, ReadValueRef};
///# fn main() -> IonResult<()> {
///
/// let mut reader = ReaderBuilder::default().build("1e0 2e0 3e0")?;
///
/// let mut sum = 0f64;
/// //                   v--- ValueReader
/// while let Some(mut value) = reader.next()? {
/// //                   v--- Interpret the data stream bytes, producing an f64
///     sum += value.read_float()?;
/// }
/// assert_eq!(sum, 6.0);
///# Ok(())
///# }
/// ```
///
/// Container variants return a handle to the container value, which you can use to traverse the
/// contents of the container.
///
/// **Traversing a list**
///
/// ```
/// use ion_rs::{IonReader, IonResult, ReaderBuilder, ValueRef, ReadValueRef};
///# fn main() -> IonResult<()> {
///
/// let mut reader = ReaderBuilder::default().build("[1, 2, 3]")?;
///
/// // We're going to calculate the list elements' total.
/// let mut sum = 0;
///
/// let mut first_element = reader.next()?.expect("first element");
/// let mut list_reader = first_element.as_list()?.reader()?;
///
/// // Visit each element in the list...
/// while let Some(mut child_value) = list_reader.next_element()? {
///   // ...and read it as an integer, adding it to the running sum.
///   sum += child_value.read_i64()?;
/// }
///
/// assert_eq!(sum, 6);
///# Ok(())
///# }
/// ```
///
/// See [ValueReader::as_list], [SequenceRef::reader], and [SequenceReader] for more information.
///
/// **Traversing a struct**
/// ```
/// use ion_rs::{IonReader, IonResult, ReaderBuilder, ValueRef, ReadValueRef};
///# fn main() -> IonResult<()> {
///
/// use ion_rs::{};
/// use ion_rs::types::integer::IntAccess;
///
/// let mut reader = ReaderBuilder::default().build("{a: 1, b: 2, c:3}")?;
///
/// let mut sum = 0;
/// let mut first_element = reader.next()?.unwrap();
/// let mut struct_reader = first_element.as_struct()?.reader()?;
///
/// while let Some(mut field) = struct_reader.next_field()? {
///   if field.read_name()? != "b" {
///     sum += field.value().read_i64()?;
///   }
/// }
///
///# Ok(())
///# }
/// ```
///
/// **Reading annotations**
///
/// The `ValueReader` can also load a value's annotations upon request.
///
/// ```
/// use ion_rs::{IonReader, IonResult, ReaderBuilder, ValueRef, ReadValueRef};
///# fn main() -> IonResult<()> {
///
/// let mut reader = ReaderBuilder::default().build("USD::29.99 GBP::25.00 JPY::30d3 EUR::27.50")?;
///
/// //                   v--- ValueReader
/// while let Some(mut value) = reader.next()? {
///   let currency = value.annotations().next().expect("currency annotation")?;
///   if currency == "EUR" {
///     let denomination = value.read_decimal()?;
///     println!("Price in euros: €{}", denomination)
///   }
/// }
///# Ok(())
///# }
/// ```
///
/// **Skip-scanning**
///
/// In this example, the reader advances through the top level of the data stream, visiting each value
/// in turn. It does not call any of [ValueReader]'s `read_*` methods, avoiding the cost of interpreting
/// those input bytes or materializing those values.
///
/// ```
/// use ion_rs::{IonReader, IonResult, ReaderBuilder, ValueRef, ReadValueRef};
///# fn main() -> IonResult<()> {
///
/// let mut reader = ReaderBuilder::default().build("foo false 8")?;
///
/// let mut count = 0;
/// //                v--- ValueReader
/// while let Some(_value) = reader.next()? {
///   // We don't call any read methods on `_value`, we just count how many we visited.
///   count += 1;
/// }
/// assert_eq!(count, 3);
///# Ok(())
///# }
/// ```
pub struct ValueReader<'r, R: RawIonReader> {
    depth: usize,
    reader: &'r mut SystemReader<R>,
}

impl<'r, R: RawIonReader> ValueReader<'r, R> {
    pub(crate) fn new(reader: &'r mut SystemReader<R>) -> ValueReader<R> {
        ValueReader {
            depth: reader.depth(),
            reader,
        }
    }

    /// Returns the [IonType] of the current value.
    pub fn ion_type(&self) -> IonType {
        self.reader
            .ion_type()
            .expect("inner 'reader' of ValueReader was not on a value")
    }

    /// Returns an iterator over this value's annotations. Each annotation is a [SymbolRef], which
    /// holds a reference to the annotation's text (if known).
    pub fn annotations(&mut self) -> impl Iterator<Item = IonResult<SymbolRef>> {
        // The system reader resolves each annotation for us
        self.reader.annotations()
    }

    /// Reads the current value from the data stream and returns it as a [ValueRef].
    pub fn read(&mut self) -> IonResult<ValueRef<R>> {
        self.reader.read_value()
    }
}

impl<'r, R: RawIonReader> Debug for ValueReader<'r, R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ValueReader ({})", self.ion_type())
    }
}

pub struct SequenceReader<'r, R: RawIonReader> {
    depth: usize,
    reader: &'r mut SystemReader<R>,
}

impl<'r, R: RawIonReader> SequenceReader<'r, R> {
    pub(crate) fn new(reader: &'r mut SystemReader<R>) -> SequenceReader<R> {
        Self {
            depth: reader.depth(),
            reader,
        }
    }

    pub fn next_element(&mut self) -> IonResult<Option<ValueReader<R>>> {
        self.reader.step_out_to_depth(self.depth)?;
        reader_for_next_value(self.reader)
    }

    pub fn read_next_element(&'r mut self) -> IonResult<Option<ValueRef<R>>> {
        self.reader.step_out_to_depth(self.depth)?;
        if advance_to_next_user_value(self.reader)?.is_none() {
            return Ok(None);
        }
        Ok(Some(self.reader.read_value()?))
    }
}

/// Provides methods to traverse the fields of a struct in the data stream.
/// ```
/// use ion_rs::{IonReader, IonResult, ReaderBuilder, ValueRef, ReadValueRef};
///# fn main() -> IonResult<()> {
///
/// let mut reader = ReaderBuilder::default().build("{a: 1, b: 2, c:3}")?;
///
/// let mut first_element = reader.next()?.unwrap();
/// let mut struct_reader = first_element
///     .as_struct()? // Verify this is a struct, get a StructRef
///     .reader()?;   // Step into the struct, get a StructReader
///
/// let mut sum = 0;
/// while let Some(mut field) = struct_reader.next_field()? {
///   if field.read_name()? != "b" {
///     sum += field.value().read_i64()?;
///   }
/// }
///
///# Ok(())
///# }
/// ```
pub struct StructReader<'r, R: RawIonReader> {
    depth: usize,
    reader: &'r mut SystemReader<R>,
}

impl<'r, R: RawIonReader> StructReader<'r, R> {
    pub(crate) fn new(reader: &'r mut SystemReader<R>) -> StructReader<R> {
        Self {
            depth: reader.depth(),
            reader,
        }
    }

    pub fn next_field(&mut self) -> IonResult<Option<FieldReader<R>>> {
        self.reader.step_out_to_depth(self.depth)?;
        if advance_to_next_user_value(self.reader)?.is_none() {
            return Ok(None);
        }
        return Ok(Some(FieldReader::new(self.reader)));
    }
}

pub struct FieldReader<'r, R: RawIonReader> {
    reader: &'r mut SystemReader<R>,
}

impl<'r, R: RawIonReader> FieldReader<'r, R> {
    pub(crate) fn new(reader: &'r mut SystemReader<R>) -> FieldReader<R> {
        Self { reader }
    }

    pub fn read_name(&mut self) -> IonResult<SymbolRef> {
        self.reader.field_name()
    }

    pub fn read_value(&mut self) -> IonResult<ValueRef<R>> {
        self.reader.read_value()
    }

    pub fn value(&mut self) -> ValueReader<R> {
        reader_for_current_value(self.reader)
    }
}

// Returns a `ValueReader` for the SystemReader's current value.
// The reader MUST be positioned on a user value before calling this method.
fn reader_for_current_value<R: RawIonReader>(reader: &mut SystemReader<R>) -> ValueReader<R> {
    // `debug_assert!` does not produce code in 'release' mode.
    debug_assert!(matches!(
        reader.current(),
        SystemStreamItem::Value(_) | SystemStreamItem::Null(_)
    ));
    ValueReader::new(reader)
}

// Skips through any/all system values until the next user value (if any) is found.
fn advance_to_next_user_value<R: RawIonReader>(
    reader: &mut SystemReader<R>,
) -> IonResult<Option<()>> {
    use crate::SystemStreamItem::*;
    loop {
        match reader.next()? {
            Nothing => return Ok(None),
            VersionMarker(_, _) | SymbolTableValue(_) | SymbolTableNull(_) => {}
            Null(_) | Value(_) => return Ok(Some(())),
        }
    }
}

// Performs `advance_to_next_user_value()` and `reader_for_current_value()` in one step, avoiding
// repetitive validation.
fn reader_for_next_value<R: RawIonReader>(
    reader: &mut SystemReader<R>,
) -> IonResult<Option<ValueReader<R>>> {
    use crate::SystemStreamItem::*;
    loop {
        match reader.next()? {
            Nothing => return Ok(None),
            VersionMarker(_, _) | SymbolTableValue(_) | SymbolTableNull(_) => {}
            Null(_) | Value(_) => return Ok(Some(ValueReader::new(reader))),
        }
    }
}
