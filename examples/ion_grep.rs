extern crate ion_rust;

use std::collections::HashSet;
use std::convert::AsRef;
use std::fs::File;
use std::io;
use std::process::exit;
use std::rc::Rc;

use crossbeam::{
    self,
    thread::{Scope, ScopedJoinHandle},
};

use failure::_core::cell::RefCell;
use memmap::MmapOptions;
use regex::bytes::Regex;

use ion_rust::result::IonResult;
use ion_rust::{
    BinaryIonCursor, Cursor, IonDataSource, IonType, Reader, SymbolTable, SymbolTableEventHandler,
};

// Functions similar to `grep -c`. Given a regular expression, recursively searches every value in
// the provided Ion stream to see if any of the text fields match that pattern.
fn main() -> IonResult<()> {
    let args: Vec<String> = std::env::args().collect();
    let args: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();

    let (mmap_enabled, pattern, path) = match args.as_slice() {
        [_, "-m", pattern, path] | [_, "--mmap", pattern, path] => (true, *pattern, *path),
        [_, pattern, path] => (false, *pattern, *path),
        _ => {
            eprintln!(
                "USAGE:\n\n    {} [-m or --mmap] <pattern> <.10n file>\n",
                args.get(0).unwrap()
            );
            exit(1);
        }
    };

    let pattern = Regex::new(pattern).unwrap_or_else(|error| {
        eprintln!("Provided pattern was not valid: {:?}", error);
        exit(2);
    });

    if mmap_enabled {
        parallel_grep(&pattern, path)?;
    } else {
        grep(&pattern, path)?;
    }
    Ok(())
}

fn show_results(num_matches: usize) {
    println!("Found {} matches", num_matches);
}

// Reads the file via a BufReader on a single thread
fn grep(pattern: &Regex, path: &str) -> IonResult<()> {
    let file = File::open(path)?;
    let buf_reader = std::io::BufReader::new(file);
    let cursor = BinaryIonCursor::new(buf_reader);
    let mut reader = Reader::new(cursor);
    let num_matches = grep_every_n(0, 1, pattern, &mut reader)?;
    show_results(num_matches);
    Ok(())
}

// mmaps the file and uses several threads to search the mapped memory in parallel
fn parallel_grep(pattern: &Regex, path: &str) -> IonResult<()> {
    let file = File::open(path)?;
    let mmap = unsafe { MmapOptions::new().map(&file)? };
    let ion_data: &[u8] = &mmap[..];
    crossbeam::scope(|scope: &Scope| {
        create_search_threads(ion_data, pattern, &scope);
    })
    .unwrap();
    Ok(())
}

// Spawns N threads to search for the provided pattern, where N is the number of CPUs on the host,
// including virtual CPUs.
fn create_search_threads<'a, 'b: 'a>(ion_data: &'b [u8], pattern: &'b Regex, scope: &Scope<'a>) {
    let num_threads = num_cpus::get();

    let mut handles: Vec<ScopedJoinHandle<IonResult<usize>>> = Vec::with_capacity(num_threads);
    for thread_number in 0..num_threads {
        let join_handle = scope.spawn(move |_| {
            create_search_thread(ion_data, num_threads, thread_number, pattern.clone())
        });
        handles.push(join_handle);
    }

    // Wait for all of the threads to finish and accumulate the results
    let mut total_matches: usize = 0;
    for handle in handles {
        total_matches += handle.join().unwrap().unwrap();
    }
    show_results(total_matches);
}

fn create_search_thread(
    ion_data: &[u8],
    num_threads: usize,
    thread_number: usize,
    pattern: Regex,
) -> IonResult<usize> {
    let io_cursor = io::Cursor::new(ion_data);
    let cursor = BinaryIonCursor::new(io_cursor);
    let mut reader = Reader::new(cursor);
    for _ in 0..thread_number {
        // Skip ahead by `thread_number` values.
        reader
            .next()
            .expect("Thread failed to skip ahead at start.");
    }
    let num_matches = sneaky_grep_every_n(thread_number, num_threads, &pattern, &mut reader)?;
    Ok(num_matches)
}

fn sneaky_grep_every_n<T: AsRef<[u8]>>(
    _thread_num: usize,
    search_every_n: usize,
    pattern: &Regex,
    reader: &mut Reader<io::Cursor<T>, BinaryIonCursor<io::Cursor<T>>>,
) -> IonResult<usize> {
    use IonType::*;

    // A ref-counted reference to a HashSet that contains the IDs of symbols that matched the pattern we're searching for.
    // If the set is empty, we don't have to search any symbols that we encounter.
    let symbol_match_cache = Rc::new(RefCell::new(HashSet::with_capacity(64)));

    // Acts as an symbol table event handler on the Reader. When new symbols are appended to the
    // table, symbol_pattern_matcher checks whether they match the pattern and if so, adds them to
    // the cache.
    let symbol_pattern_matcher = SymbolPatternMatcher {
        pattern: pattern.clone(),
        cache: symbol_match_cache.clone(),
    };
    reader.symtab_event_handler(symbol_pattern_matcher);

    let number_to_skip = search_every_n - 1;
    let mut top_level_value_count = 0;
    let mut match_count: usize = 0;
    let mut string_match_count: usize = 0;
    let mut symbol_match_count: usize = 0;
    let mut field_match_count: usize = 0;
    let mut item_matched = false;
    'top_level: loop {
        if reader.depth() == 0 {
            // The reader is between top-level values
            item_matched = false;
            if top_level_value_count % search_every_n != 0 {
                // Skip the next (search_every_n - 1) values because other threads will be handling those.
                for _ in 0..number_to_skip {
                    if reader.next()?.is_none() {
                        break 'top_level;
                    }
                }
                top_level_value_count += number_to_skip;
                continue;
            }
            top_level_value_count += 1;
        } else {
            // The reader is positioned within a container
            if item_matched {
                // The last value we looked at in this item matched, so we don't need to search the
                // rest of it. Call step_out() until we've reached the top level.

                reader.step_out()?;
                while reader.depth() > 0 {
                    reader.step_out()?;
                }
                continue;
            }
        }

        // Read the next value
        let ion_type = match reader.next()? {
            Some((_, true)) => continue, // Skip nulls
            Some((ion_type, false)) => ion_type,
            None if reader.depth() > 0 => {
                // We've run out of values in this container.
                reader.step_out()?;
                continue;
            }
            None => {
                // We've run out of values at the top level, so we're done.
                break;
            }
        };

        let cache = symbol_match_cache.borrow();

        // If this is a top-level value and there aren't any symbols that match,
        // we can pattern-match against the value's raw bytes without parsing them as Ion.
        if reader.depth() == 0 && cache.len() == 0 {
            let bytes = reader.raw_value_bytes().unwrap();
            if !pattern.is_match(bytes) {
                continue;
            }
        }

        // If there's a field ID (i.e. we're inside a struct)...
        let field_name_matches = reader
            .field_id()
            .map(|sid| {
                // ... see if our matched symbol cache has the ID in question.
                cache.len() > 0 && cache.contains(&sid)
            })
            .unwrap_or(false);
        if field_name_matches {
            match_count += 1;
            field_match_count += 1;
            item_matched = true;
            continue;
        }

        match ion_type {
            Struct | List | SExpression => reader.step_in()?,
            String => {
                let depth = reader.depth();
                let field_name = reader.field_name().map(|s| s.to_string());
                let matches = reader
                    .string_bytes_map(|buf| pattern.is_match(buf))?
                    .unwrap_or(false);
                if matches {
                    match_count += 1;
                    string_match_count += 1;
                    item_matched = true;
                }
            }
            Symbol => {
                if cache.len() > 0 {
                    let sid = reader.read_symbol_id()?.unwrap();
                    if cache.contains(&sid) {
                        match_count += 1;
                        symbol_match_count += 1;
                        item_matched = true;
                    }
                }
            }
            _ => {}
        }
    }
    println!("Strings: {}, Symbols: {}, Fields: {}", string_match_count, symbol_match_count, field_match_count);
    Ok(match_count)
}

// Searches a top level value for the provided pattern, then skips (search_every_n - 1) top level
// values. This allows `search_every_n` threads to search the complete stream without synchronization.
fn grep_every_n<R: IonDataSource, C: Cursor<R>>(
    _thread_num: usize,
    search_every_n: usize,
    pattern: &Regex,
    reader: &mut Reader<R, C>,
) -> IonResult<usize> {
    use IonType::*;

    // A ref-counted reference to a HashSet that contains the IDs of symbols that matched the pattern we're searching for.
    // If the set is empty, we don't have to search any symbols that we encounter.
    let symbol_match_cache = Rc::new(RefCell::new(HashSet::with_capacity(64)));

    // Acts as an symbol table event handler on the Reader. When new symbols are appended to the
    // table, symbol_pattern_matcher checks whether they match the pattern and if so, adds them to
    // the cache.
    let symbol_pattern_matcher = SymbolPatternMatcher {
        pattern: pattern.clone(),
        cache: symbol_match_cache.clone(),
    };
    reader.symtab_event_handler(symbol_pattern_matcher);

    let number_to_skip = search_every_n - 1;
    let mut top_level_value_count = 0;
    let mut match_count: usize = 0;
    let mut string_match_count: usize = 0;
    let mut symbol_match_count: usize = 0;
    let mut field_match_count: usize = 0;
    let mut item_matched = false;
    'top_level: loop {
        if reader.depth() == 0 {
            // The reader is between top-level values
            item_matched = false;
            if top_level_value_count % search_every_n != 0 {
                // Skip the next (search_every_n - 1) values because other threads will be handling those.
                for _ in 0..number_to_skip {
                    if reader.next()?.is_none() {
                        break 'top_level;
                    }
                }
                top_level_value_count += number_to_skip;
                continue;
            }
            top_level_value_count += 1;
        } else {
            // The reader is positioned within a container
            if item_matched {
                // The last value we looked at in this item matched, so we don't need to search the
                // rest of it. Call step_out() until we've reached the top level.
                reader.step_out()?;
                while reader.depth() > 0 {
                    reader.step_out()?;
                }
                item_matched = false; // <-- this section is busted.
            }
        }

        // Read the next value
        let ion_type = match reader.next()? {
            Some((_, true)) => continue, // Skip nulls
            Some((ion_type, false)) => ion_type,
            None if reader.depth() > 0 => {
                // We've run out of values in this container.
                reader.step_out()?;
                continue;
            }
            None => {
                // We've run out of values at the top level, so we're done.
                break;
            }
        };

        // If there's a field ID (i.e. we're inside a struct)...
        let field_name_matches = reader
            .field_id()
            .map(|sid| {
                // ... see if our matched symbol cache has the ID in question.
                let cache = symbol_match_cache.borrow();
                cache.len() > 0 && cache.contains(&sid)
            })
            .unwrap_or(false);
        if field_name_matches {
            match_count += 1;
            field_match_count += 1;
            item_matched = true;
            continue;
        }

        match ion_type {
            Struct | List | SExpression => reader.step_in()?,
            String => {
                let matches = reader
                    .string_bytes_map(|buf| pattern.is_match(buf))?
                    .unwrap_or(false);
                if matches {
                    match_count += 1;
                    string_match_count += 1;
                    item_matched = true;
                }
            }
            Symbol => {
                let cache = symbol_match_cache.borrow();
                if cache.len() > 0 {
                    let sid = reader.read_symbol_id()?.unwrap();
                    if cache.contains(&sid) {
                        match_count += 1;
                        symbol_match_count += 1;
                        item_matched = true;
                    }
                }
            }
            _ => {}
        }
    }
    println!("Strings: {}, Symbols: {}, Fields: {}", string_match_count, symbol_match_count, field_match_count);
    Ok(match_count)
}

struct SymbolPatternMatcher {
    pattern: Regex,
    cache: Rc<RefCell<HashSet<usize>>>,
}

// Responds to changes in the symbol table by adding the IDs of any symbols with matching text to
// the cache.
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
