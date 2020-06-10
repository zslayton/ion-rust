extern crate ion_rust;

use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::process::exit;
use std::rc::Rc;

use failure::_core::cell::RefCell;
use memmap::MmapOptions;
use regex::bytes::Regex;

use ion_rust::result::IonResult;
use ion_rust::{
    BinaryIonCursor, Cursor, IonDataSource, IonType, Reader, SymbolTable, SymbolTableEventHandler,
};

fn bail(args: &Vec<String>, text: &str, status: i32) -> ! {
    eprintln!(
        "USAGE:\n\n    {} [pattern] [Binary Ion file]\n",
        args.get(0).unwrap()
    );
    eprintln!("{}", text);
    exit(status);
}

fn main() -> IonResult<()> {
    let args: Vec<String> = std::env::args().collect();
    let pattern = args.get(1).unwrap_or_else(|| {
        bail(&args, "No pattern was specified.", 1);
    });

    let path = args.get(2).unwrap_or_else(|| {
        bail(&args, "No input file was specified.", 2);
    });

    let use_mmap = args.get(3).map(|a| a == "--mmap").unwrap_or(false);

    let pattern = Regex::new(pattern).unwrap_or_else(|error| {
        bail(
            &args,
            &format!("Provided pattern was not valid: {:?}", error),
            3,
        );
    });

    let file = File::open(path)?;

    let number_of_matches: usize;
    if use_mmap {
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        let bytes_cursor = io::Cursor::new(mmap);
        let cursor = BinaryIonCursor::new(bytes_cursor);
        let mut reader = Reader::new(cursor);
        number_of_matches = grep(pattern, &mut reader)?;
    } else {
        let buf_reader = std::io::BufReader::new(file);
        let cursor = BinaryIonCursor::new(buf_reader);
        let mut reader = Reader::new(cursor);
        number_of_matches = grep(pattern, &mut reader)?;
    }
    println!("Found {} matches", number_of_matches);
    Ok(())
}

struct SymbolPatternMatcher {
    pattern: Regex,
    cache: Rc<RefCell<HashSet<usize>>>,
}

impl SymbolTableEventHandler for SymbolPatternMatcher {
    fn on_reset(&mut self, symbol_table: &SymbolTable) {
        let mut cache = self.cache.borrow_mut();
        for (id, symbol) in symbol_table.symbols().iter().enumerate() {
            if self.pattern.is_match(symbol.as_bytes()) {
                cache.insert(id);
            }
        }
    }

    fn on_append(&mut self, symbol_table: &SymbolTable, starting_id: usize) {
        let mut cache = self.cache.borrow_mut();
        let symbols = symbol_table.symbols_tail(starting_id);
        for (i, symbol) in symbols.iter().enumerate() {
            let id = starting_id + i;
            if self.pattern.is_match(symbol.as_bytes()) {
                cache.insert(id);
            }
        }
    }
}

fn grep<R: IonDataSource, C: Cursor<R>>(
    pattern: Regex,
    reader: &mut Reader<R, C>,
) -> IonResult<usize> {
    use IonType::*;

    let symbol_match_cache = Rc::new(RefCell::new(HashSet::with_capacity(64)));
    let symbol_pattern_matcher = SymbolPatternMatcher {
        pattern: pattern.clone(),
        cache: symbol_match_cache.clone(),
    };

    reader.symtab_event_handler(symbol_pattern_matcher);

    let mut count: usize = 0;
    loop {
        let ion_type = match reader.next()? {
            Some((_, true)) => continue,
            Some((ion_type, false)) => ion_type,
            None if reader.depth() > 0 => {
                reader.step_out()?;
                continue;
            }
            None => break,
        };

        let cache = symbol_match_cache.borrow();

        if cache.len() > 0 {
            let field_name_matches = reader
                .field_id()
                .map(|sid| cache.contains(&sid))
                .unwrap_or(false);
            if field_name_matches {
                count += 1;
                reader.step_out()?;
                continue;
            }
        }
        let mut item_matched = false;
        match ion_type {
            Struct | List | SExpression => reader.step_in()?,
            String => {
                let matches = reader
                    .string_bytes_map(|buf| pattern.is_match(buf))?
                    .unwrap_or(false);
                if matches {
                    count += 1;
                    item_matched = true;
                }
            }
            Symbol => {
                if cache.len() > 0 {
                    let sid = reader.read_symbol_id()?.unwrap();
                    if cache.contains(&sid) {
                        count += 1;
                        item_matched = true;
                    }
                }
            }
            _ => {}
        }
        if item_matched {
            // This item matched, so we don't need to search the rest of it.
            if reader.depth() > 0 {
                reader.step_out()?;
            }
        }
    }
    Ok(count)
}
