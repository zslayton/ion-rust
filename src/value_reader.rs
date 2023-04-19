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

    pub fn step_in(self) -> IonResult<StructReader<'r, R>> {
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

    pub fn step_in(self) -> IonResult<SequenceReader<'r, R>> {
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

pub struct ValueReader<'r, R: RawIonReader> {
    reader: &'r mut SystemReader<R>,
}

impl<'r, R: RawIonReader> ValueReader<'r, R> {
    pub(crate) fn new(reader: &'r mut SystemReader<R>) -> ValueReader<R> {
        ValueReader { reader }
    }

    pub fn ion_type(&self) -> IonType {
        self.reader
            .ion_type()
            .expect("inner 'reader' of ValueReader was not on a value")
    }

    // TODO: step_out() on Drop?
    pub fn annotations<'a>(&'a mut self) -> impl Iterator<Item = IonResult<SymbolRef>> + 'a {
        // The system reader resolves each annotation for us
        self.reader.annotations()
    }

    pub fn read(&mut self) -> IonResult<ValueRef<R>> {
        self.reader.read_value()
    }
}

impl<'r, R: RawIonReader> Debug for ValueReader<'r, R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ValueReader ({})", self.ion_type())
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

pub struct StructReader<'r, R: RawIonReader> {
    reader: &'r mut SystemReader<R>,
}

impl<'r, R: RawIonReader> StructReader<'r, R> {
    pub(crate) fn new(reader: &'r mut SystemReader<R>) -> StructReader<R> {
        Self { reader }
    }

    pub fn next_field(&mut self) -> IonResult<Option<FieldReader<R>>> {
        if let None = advance_to_next_user_value(self.reader)? {
            return Ok(None);
        }
        return Ok(Some(FieldReader::new(self.reader)));
    }

    pub fn step_out(self) -> IonResult<()> {
        self.reader.step_out()
    }
}

pub struct SequenceReader<'r, R: RawIonReader> {
    reader: &'r mut SystemReader<R>,
}

impl<'r, R: RawIonReader> SequenceReader<'r, R> {
    pub(crate) fn new(reader: &'r mut SystemReader<R>) -> SequenceReader<R> {
        Self { reader }
    }

    pub fn next_element(&mut self) -> IonResult<Option<ValueReader<R>>> {
        reader_for_next_value(self.reader)
    }

    pub fn read_next_element(&mut self) -> IonResult<Option<ValueRef<R>>> {
        if let None = advance_to_next_user_value(self.reader)? {
            return Ok(None);
        }
        Ok(Some(self.reader.read_value()?))
    }

    pub fn step_out(self) -> IonResult<()> {
        self.reader.step_out()
    }
}

// The reader MUST be positioned on a user value before calling this method.
fn reader_for_current_value<R: RawIonReader>(reader: &mut SystemReader<R>) -> ValueReader<R> {
    debug_assert!(matches!(
        reader.current(),
        SystemStreamItem::Value(_) | SystemStreamItem::Null(_)
    ));
    ValueReader { reader }
}

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

fn reader_for_next_value<R: RawIonReader>(
    reader: &mut SystemReader<R>,
) -> IonResult<Option<ValueReader<R>>> {
    use crate::SystemStreamItem::*;
    loop {
        match reader.next()? {
            Nothing => return Ok(None),
            VersionMarker(_, _) | SymbolTableValue(_) | SymbolTableNull(_) => {}
            Null(_) | Value(_) => return Ok(Some(ValueReader { reader })),
        }
    }
}
