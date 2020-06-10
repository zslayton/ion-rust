use crate::cursor::StreamItem::*;
use crate::result::IonResult;
use crate::symbol_table::SymbolTable;
use crate::types::SymbolId;
use crate::{Cursor, IonDataSource, IonType};
use bigdecimal::BigDecimal;
use chrono::{DateTime, FixedOffset};
use delegate::delegate;
use std::marker::PhantomData;

pub struct Reader<D: IonDataSource, C: Cursor<D>> {
    cursor: C,
    symbol_table: SymbolTable,
    spooky: PhantomData<D>,
}

impl<D: IonDataSource, C: Cursor<D>> Reader<D, C> {
    pub fn new(cursor: C) -> Reader<D, C> {
        Reader {
            cursor,
            symbol_table: SymbolTable::new(),
            spooky: PhantomData,
        }
    }

    pub fn next(&mut self) -> IonResult<Option<(IonType, bool)>> {
        loop {
            match self.cursor.next()? {
                Some(VersionMarker) => {
                    self.symbol_table.reset();
                }
                Some(Value(IonType::Struct, false)) => {
                    // TODO: Replace with fn
                    if let [3, ..] = self.cursor.annotation_ids() {
                        self.read_symbol_table()?;
                    } else {
                        return Ok(Some((IonType::Struct, false)));
                    }
                }
                Some(Value(ion_type, is_null)) => return Ok(Some((ion_type, is_null))),
                None => return Ok(None),
            }
        }
    }

    fn read_symbol_table(&mut self) -> IonResult<()> {
        self.cursor.step_in()?;

        let mut is_append = false;
        let mut new_symbols = vec![];
        while let Some(Value(ion_type, is_null)) = self.cursor.next()? {
            let field_id = self
                .cursor
                .field_id()
                .expect("No field ID found inside $ion_symbol_table struct.");
            match (field_id, ion_type, is_null) {
                (6, IonType::Symbol, false) => {
                    // imports
                    if self.cursor.read_symbol_id()?.unwrap() != 3 {
                        unimplemented!("Can't handle non-3 symbols value.");
                    }
                    is_append = true;
                }
                (7, IonType::List, false) => {
                    // symbols
                    self.cursor.step_in()?;
                    while let Some(Value(IonType::String, false)) = self.cursor.next()? {
                        let text = self.cursor.read_string()?.unwrap();
                        new_symbols.push(text);
                    }
                    self.cursor.step_out()?;
                }
                something_else => {
                    unimplemented!("No support for {:?}", something_else);
                }
            }
            if !is_append {
                self.symbol_table.reset();
                println!("Resetting symbol table.");
            }
            for new_symbol in new_symbols.drain(..) {
                // print!("{}", new_symbol);
                let _id = self.symbol_table.intern(new_symbol);
                // println!(" (${})", id);
            }
        }

        self.cursor.step_out()?;
        Ok(())
    }

    pub fn field_name(&self) -> Option<&str> {
        if let Some(id) = self.cursor.field_id() {
            return self.symbol_table.text_for(id);
        }
        None
    }

    pub fn symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }

    // Any method listed here will be delegated to self.cursor.
    delegate! {
        to self.cursor {
            pub fn ion_version(&self) -> (u8, u8);
            pub fn ion_type(&self) -> Option<IonType>;
            pub fn annotation_ids(&self) -> &[SymbolId];
            pub fn field_id(&self) -> Option<SymbolId>;
            pub fn read_null(&mut self) -> IonResult<Option<IonType>>;
            pub fn read_bool(&mut self) -> IonResult<Option<bool>>;
            pub fn read_i64(&mut self) -> IonResult<Option<i64>>;
            pub fn read_f32(&mut self) -> IonResult<Option<f32>>;
            pub fn read_f64(&mut self) -> IonResult<Option<f64>>;
            pub fn read_big_decimal(&mut self) -> IonResult<Option<BigDecimal>>;
            pub fn read_string(&mut self) -> IonResult<Option<String>>;
            pub fn string_ref_map<F, T>(&mut self, f: F) -> IonResult<Option<T>> where F: FnOnce(&str) -> T;
            pub fn string_bytes_map<F, T>(&mut self, f: F) -> IonResult<Option<T>> where F: FnOnce(&[u8]) -> T;
            pub fn read_symbol_id(&mut self) -> IonResult<Option<SymbolId>>;
            pub fn read_blob_bytes(&mut self) -> IonResult<Option<Vec<u8>>>;
            pub fn read_clob_bytes(&mut self) -> IonResult<Option<Vec<u8>>>;
            pub fn read_datetime(&mut self) -> IonResult<Option<DateTime<FixedOffset>>>;
            pub fn step_in(&mut self) -> IonResult<()>;
            pub fn step_out(&mut self) -> IonResult<()>;
            pub fn depth(&self) -> usize;
        }
    }
}
