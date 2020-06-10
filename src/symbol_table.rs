use std::collections::HashMap;

use crate::constants::v1_0;
use crate::types::SymbolId;

pub struct SymbolTable {
    symbols_by_id: Vec<String>,
    ids_by_text: HashMap<String, SymbolId>,
}

impl SymbolTable {
    pub fn new() -> SymbolTable {
        let mut symbol_table = SymbolTable {
            symbols_by_id: Vec::with_capacity(v1_0::SYSTEM_SYMBOLS.len()),
            ids_by_text: HashMap::new(),
        };
        symbol_table.initialize();
        symbol_table
    }

    fn initialize(&mut self) {
        for (id, text) in v1_0::SYSTEM_SYMBOLS.iter().enumerate() {
            self.symbols_by_id.push(text.to_string());
            self.ids_by_text.insert(text.to_string(), id);
        }
    }

    pub fn reset(&mut self) {
        self.symbols_by_id.clear();
        self.ids_by_text.clear();
        self.initialize();
    }

    pub fn intern(&mut self, text: String) -> SymbolId {
        // If the text is already in the symbol table, return the ID associated with it.
        if let Some(id) = self.ids_by_text.get(&text) {
            return *id;
        }

        // Otherwise, intern it and return the new ID.
        let id = self.symbols_by_id.len();
        self.symbols_by_id.push(text.to_string());
        self.ids_by_text.insert(text, id);
        id
    }

    pub fn sid_for<A: AsRef<str>>(&self, text: &A) -> Option<SymbolId> {
        self.ids_by_text.get(text.as_ref()).copied()
    }

    pub fn text_for(&self, sid: usize) -> Option<&str> {
        self.symbols_by_id.get(sid).map(|text| text.as_str())
    }

    pub fn symbols(&self) -> &[String] {
        &self.symbols_by_id
    }

    pub fn symbols_tail(&self, start: usize) -> &[String] {
        &self.symbols_by_id[start..]
    }

    pub fn len(&self) -> usize {
        self.symbols_by_id.len()
    }
}

pub trait SymbolTableEventHandler {
    fn on_reset<'a>(&'a mut self, symbol_table: &'a SymbolTable);
    fn on_append<'a>(&'a mut self, symbol_table: &'a SymbolTable, starting_id: usize);
}
