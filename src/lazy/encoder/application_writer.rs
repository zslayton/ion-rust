use crate::constants::v1_0::system_symbol_ids;
use crate::lazy::encoder::value_writer::internal::MakeValueWriter;
use crate::lazy::encoder::value_writer::{
    AnnotatableValueWriter, SequenceWriter, StructWriter, ValueWriter,
};
use crate::lazy::encoder::write_as_ion::WriteAsIon;
use crate::lazy::encoder::{LazyEncoder, LazyRawWriter};
use crate::raw_symbol_token_ref::AsRawSymbolTokenRef;
use crate::{Decimal, Int, IonResult, IonType, RawSymbolTokenRef, Symbol, SymbolTable, Timestamp};
use delegate::delegate;
use std::io::Write;

pub struct ApplicationWriter<E: LazyEncoder, W: Write> {
    symbol_table: SymbolTable,
    num_symbols_last_flush: usize,
    // Writes encoding directives (e.g. symbol tables) to an in-memory buffer
    directive_writer: E::Writer<Vec<u8>>,
    // Writes user data to an in-memory buffer
    data_writer: E::Writer<Vec<u8>>,
    // The sink to which all data will be written each time that `flush()` is called
    output: W,
}

impl<E: LazyEncoder, W: Write> ApplicationWriter<E, W> {
    pub fn annotatable_value_writer(&mut self) -> ApplicationAnnotatableValueWriter<'_, E, W> {
        ApplicationAnnotatableValueWriter { writer: self }
    }
}

impl<E: LazyEncoder, W: Write> ApplicationWriter<E, W> {
    fn new(output: W) -> IonResult<Self> {
        let symbol_table = SymbolTable::new();
        let num_symbols_last_flush = symbol_table.len();
        let mut directive_writer = E::Writer::new(Vec::new()).unwrap();
        let mut data_writer = E::Writer::new(Vec::new())?;
        // Suppress the data writer's IVM; TODO: a better abstraction for this
        data_writer.output().clear();
        Ok(ApplicationWriter {
            symbol_table,
            num_symbols_last_flush,
            data_writer,
            directive_writer,
            output,
        })
    }

    fn write<V: WriteAsIon>(&mut self, value: V) -> IonResult<&mut Self> {
        value.write_as_ion(self.annotatable_value_writer())?;
        Ok(self)
    }

    fn flush(&mut self) -> IonResult<()> {
        let Self {
            symbol_table,
            num_symbols_last_flush,
            directive_writer,
            data_writer,
            output,
        } = self;
        let num_new_symbols = symbol_table.len() - *num_symbols_last_flush;
        let new_symbols = symbol_table
            .symbols_tail(*num_symbols_last_flush)
            .iter()
            .map(Symbol::text)
            // TODO: impl WriteAsIonValue for slice/vec iterators so allocation isn't necessary
            .collect::<Vec<_>>();
        if num_new_symbols > 0 {
            directive_writer
                .value_writer()
                .with_annotations(&[system_symbol_ids::ION_SYMBOL_TABLE])
                .write_struct(|s| {
                    s.write(
                        system_symbol_ids::IMPORTS.as_raw_symbol_token_ref(),
                        system_symbol_ids::ION_SYMBOL_TABLE.as_raw_symbol_token_ref(),
                    )?
                    .write(
                        system_symbol_ids::SYMBOLS.as_raw_symbol_token_ref(),
                        new_symbols,
                    )?;
                    Ok(())
                })?;
            directive_writer.flush()?;
            *num_symbols_last_flush = symbol_table.len();
        }
        if !directive_writer.output().is_empty() {
            output.write_all(directive_writer.output().as_slice())?;
            directive_writer.output().clear();
        }

        data_writer.flush()?;
        let user_data = data_writer.output().as_slice();
        output.write_all(user_data)?;
        data_writer.output().clear();
        Ok(())
    }
}

impl<E: LazyEncoder, W: Write> MakeValueWriter for ApplicationWriter<E, W> {
    type ValueWriter<'a> = ApplicationAnnotatableValueWriter<'a, E, W> where Self: 'a;

    fn value_writer(&mut self) -> Self::ValueWriter<'_> {
        self.annotatable_value_writer()
    }
}

impl<E: LazyEncoder, W: Write> SequenceWriter for ApplicationWriter<E, W> {}

pub struct ApplicationValueWriter<'a, E: LazyEncoder, W: Write> {
    writer: &'a mut ApplicationWriter<E, W>,
}

impl<'value, E: LazyEncoder, W: Write> ValueWriter for ApplicationValueWriter<'value, E, W> {
    type ListWriter<'a> = ApplicationListWriter<'value, E, W>;
    type SExpWriter<'a> = ApplicationListWriter<'value, E, W>;
    type StructWriter<'a> = ApplicationListWriter<'value, E, W>;

    delegate! {
        to self.writer.data_writer.value_writer() {
            fn write_null(self, ion_type: IonType) -> IonResult<()>;
            fn write_bool(self, value: bool) -> IonResult<()>;
            fn write_i64(self, value: i64) -> IonResult<()>;
            fn write_int(self, value: &Int) -> IonResult<()>;
            fn write_f32(self, value: f32) -> IonResult<()>;
            fn write_f64(self, value: f64) -> IonResult<()>;
            fn write_decimal(self, value: &Decimal) -> IonResult<()>;
            fn write_timestamp(self, value: &Timestamp) -> IonResult<()>;
            fn write_string(self, value: impl AsRef<str>) -> IonResult<()>;
            fn write_clob(self, value: impl AsRef<[u8]>) -> IonResult<()>;
            fn write_blob(self, value: impl AsRef<[u8]>) -> IonResult<()>;
        }
    }

    fn write_symbol(self, value: impl AsRawSymbolTokenRef) -> IonResult<()> {
        // XXX: This immutable variable is declared here to extend its lifetime.
        let token;
        let symbol = match value.as_raw_symbol_token_ref() {
            // The symbol to write is a symbol table index
            symbol_id @ RawSymbolTokenRef::SymbolId(_) => symbol_id,
            // The symbol to write is text and the current encoding doesn't require us to intern it
            symbol @ RawSymbolTokenRef::Text(_) if E::SUPPORTS_TEXT_SYMBOL_TOKENS => symbol,
            // The symbol to write is text and the current encoding requires us to intern it
            RawSymbolTokenRef::Text(symbol) => {
                // If the text is already interned, write the corresponding symbol table index
                token = self
                    .writer
                    .symbol_table
                    .intern(symbol.as_ref())
                    .as_raw_symbol_token_ref();
                // `token` borrows from the temporary `sid` and so cannot escape this scope,
                // so we just go ahead and write it and return.
                token
            }
        };

        self.writer.data_writer.write(symbol)?;
        Ok(())
    }

    fn write_list<F: for<'a> FnOnce(&mut Self::ListWriter<'a>) -> IonResult<()>>(
        self,
        list_fn: F,
    ) -> IonResult<()> {
        todo!()
    }

    fn write_sexp<F: for<'a> FnOnce(&mut Self::SExpWriter<'a>) -> IonResult<()>>(
        self,
        sexp_fn: F,
    ) -> IonResult<()> {
        todo!()
    }

    fn write_struct<F: for<'a> FnOnce(&mut Self::StructWriter<'a>) -> IonResult<()>>(
        self,
        struct_fn: F,
    ) -> IonResult<()> {
        todo!()
    }
}

pub struct ApplicationListWriter<'value, E: LazyEncoder, W: Write> {
    writer: &'value mut ApplicationWriter<E, W>,
}

impl<'value, E: LazyEncoder, W: Write> MakeValueWriter for ApplicationListWriter<'value, E, W> {
    type ValueWriter<'a> = ApplicationAnnotatableValueWriter<'a, E, W>
    where
        Self: 'a;

    fn value_writer(&mut self) -> Self::ValueWriter<'_> {
        ApplicationAnnotatableValueWriter {
            writer: self.writer,
        }
    }
}

impl<'value, E: LazyEncoder, W: Write> SequenceWriter for ApplicationListWriter<'value, E, W> {}

impl<'value, E: LazyEncoder, W: Write> StructWriter for ApplicationListWriter<'value, E, W> {
    fn write<A: AsRawSymbolTokenRef, V: WriteAsIon>(
        &mut self,
        name: A,
        value: V,
    ) -> IonResult<&mut Self> {
        todo!()
    }
}

pub struct ApplicationAnnotatableValueWriter<'value, E: LazyEncoder, W: Write> {
    writer: &'value mut ApplicationWriter<E, W>,
}

impl<'value, E: LazyEncoder, W: Write> AnnotatableValueWriter
    for ApplicationAnnotatableValueWriter<'value, E, W>
{
    type ValueWriter = ApplicationValueWriter<'value, E, W>;
    type AnnotatedValueWriter<'a, SymbolType: AsRawSymbolTokenRef + 'a> = ApplicationAnnotatedValueWriter<'a, E, W, SymbolType>
        where
            Self: 'a;

    fn with_annotations<'a, SymbolType: AsRawSymbolTokenRef>(
        self,
        annotations: &'a [SymbolType],
    ) -> Self::AnnotatedValueWriter<'a, SymbolType>
    where
        Self: 'a,
    {
        todo!()
    }

    fn without_annotations(self) -> Self::ValueWriter {
        ApplicationValueWriter {
            writer: self.writer,
        }
    }
}

pub struct ApplicationAnnotatedValueWriter<
    'value,
    E: LazyEncoder,
    W: Write,
    SymbolType: AsRawSymbolTokenRef,
> {
    annotations: &'value [SymbolType],
    writer: &'value mut ApplicationWriter<E, W>,
}

impl<'value, E: LazyEncoder, W: Write, SymbolType: AsRawSymbolTokenRef>
    ApplicationAnnotatedValueWriter<'value, E, W, SymbolType>
{
    pub(crate) fn value_writer(self) -> ApplicationValueWriter<'value, E, W> {
        ApplicationValueWriter {
            writer: self.writer,
        }
    }

    fn detect_new_symbols_in_annotations(&mut self) -> IonResult<()> {
        if E::SUPPORTS_TEXT_SYMBOL_TOKENS {
            // This method is a NOP for (e.g.) text Ion 1.0, which doesn't usually maintain a symbol table.
            // This branch condition could be relaxed for different writer configurations.
            return Ok(());
        }
        for annotation in self.annotations {
            if let RawSymbolTokenRef::Text(cow_str) = annotation.as_raw_symbol_token_ref() {
                let text = cow_str.as_ref();
                // TODO: Look at adding an `AnnotatedValueWriter` trait that can return an `impl Iterator<Item=SymbolType>`.
                //       RPITIT will be stable in late December.
                //       Without an iterator, we have to solve the question of how to encode EITHER the tokens that
                //       the user specified OR the symbol ID tokens to which they've mapped.
                self.writer.symbol_table.intern(text)
            }
        }
        Ok(())
    }
}

impl<'value, E: LazyEncoder, W: Write, SymbolType: AsRawSymbolTokenRef> ValueWriter
    for ApplicationAnnotatedValueWriter<'value, E, W, SymbolType>
{
    type ListWriter<'a> = ApplicationListWriter<'value, E, W>;
    type SExpWriter<'a> = ApplicationListWriter<'value, E, W>;
    type StructWriter<'a> = ApplicationListWriter<'value, E, W>;

    delegate! {
        to self.value_writer() {
            fn write_null(self, ion_type: IonType) -> IonResult<()>;
            fn write_bool(self, value: bool) -> IonResult<()>;
            fn write_i64(self, value: i64) -> IonResult<()>;
            fn write_int(self, value: &Int) -> IonResult<()>;
            fn write_f32(self, value: f32) -> IonResult<()>;
            fn write_f64(self, value: f64) -> IonResult<()>;
            fn write_decimal(self, value: &Decimal) -> IonResult<()>;
            fn write_timestamp(self, value: &Timestamp) -> IonResult<()>;
            fn write_string(self, value: impl AsRef<str>) -> IonResult<()>;
            fn write_symbol(self, value: impl AsRawSymbolTokenRef) -> IonResult<()>;
            fn write_clob(self, value: impl AsRef<[u8]>) -> IonResult<()>;
            fn write_blob(self, value: impl AsRef<[u8]>) -> IonResult<()>;
            fn write_list<F: for<'a> FnOnce(&mut Self::ListWriter<'a>) -> IonResult<()>>(
                self,
                list_fn: F,
            ) -> IonResult<()>;
            fn write_sexp<F: for<'a> FnOnce(&mut Self::SExpWriter<'a>) -> IonResult<()>>(
                self,
                sexp_fn: F,
            ) -> IonResult<()>;
            fn write_struct<
                F: for<'a> FnOnce(&mut Self::StructWriter<'a>) -> IonResult<()>,
            >(
                self,
                struct_fn: F,
            ) -> IonResult<()>;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lazy::encoder::application_writer::ApplicationWriter;
    use crate::lazy::encoder::value_writer::SequenceWriter;
    use crate::lazy::encoding::BinaryEncoding_1_0;
    use crate::{Element, IonResult};

    #[test]
    fn write_int() -> IonResult<()> {
        let mut buffer = Vec::new();
        let mut writer = ApplicationWriter::<BinaryEncoding_1_0, _>::new(&mut buffer)?;
        // Writing an int, so no interception has to happen
        writer.write(22)?.flush()?;
        let element = Element::read_one(buffer.as_slice())?;
        println!("{element}");
        Ok(())
    }

    #[test]
    fn write_symbols() -> IonResult<()> {
        let mut buffer = Vec::new();
        let mut writer = ApplicationWriter::<BinaryEncoding_1_0, _>::new(&mut buffer)?;
        // Writing an int, so no interception has to happen
        writer
            .write_symbol("foo")?
            .write_symbol("bar")?
            .write_symbol("baz")?
            .flush()?;
        let elements = Element::read_all(buffer.as_slice())?;
        std::fs::write("/tmp/out.ion", buffer.as_slice()).unwrap();
        println!("{elements:?}");
        Ok(())
    }
}
