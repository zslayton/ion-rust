extern crate ion_rust;

use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::process::exit;
use std::rc::Rc;

use crossbeam::{self, thread::{Scope, ScopedJoinHandle}};

use failure::_core::cell::RefCell;
use memmap::MmapOptions;
use regex::bytes::Regex;

use ion_rust::result::{IonResult, io_error};
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

    let use_mmap = args.get(3).map(|a| a == "mmap").unwrap_or(false);

    let pattern = Regex::new(pattern).unwrap_or_else(|error| {
        bail(
            &args,
            &format!("Provided pattern was not valid: {:?}", error),
            3,
        );
    });

    if use_mmap {
        parallel_grep(&pattern, path.as_str())?;
    } else {
        grep(pattern, path.as_str())?;
    }
    Ok(())
}

fn show_results(num_matches: usize) {
    println!("Found {} matches", num_matches);
}

fn grep(
    pattern: Regex,
    // reader: &mut Reader<R, C>,
    path: &str
) -> IonResult<()> {
    let file = File::open(path)?;
    let buf_reader = std::io::BufReader::new(file);
    let cursor = BinaryIonCursor::new(buf_reader);
    let mut reader = Reader::new(cursor);
    let num_matches = grep_every_n(0, 1, pattern, &mut reader)?;
    show_results(num_matches);
    Ok(())
}

fn parallel_grep(pattern: &Regex, path: &str) -> IonResult<()> {
    let file = File::open(path)?;
    let mmap = unsafe { MmapOptions::new().map(&file)? };
    let ion_data: &[u8] = &mmap[..];
    crossbeam::scope(|scope: &Scope| {
        create_search_threads(ion_data, pattern, &scope);
    }).unwrap();
    Ok(())
}

fn create_search_threads<'a, 'b: 'a>(ion_data: &'b [u8], pattern: &'b Regex, scope: &Scope<'a>) {
    let num_threads = num_cpus::get();

    let mut handles: Vec<ScopedJoinHandle<IonResult<usize>>> = Vec::with_capacity(num_threads);

    println!("#threads: {}", num_threads);
    // Spawn several threads running `grep_every_n`
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

fn create_search_thread(ion_data: &[u8], num_threads: usize, thread_number: usize, pattern: Regex) -> IonResult<usize> {
    let io_cursor = io::Cursor::new(ion_data);
    let cursor = BinaryIonCursor::new(io_cursor);
    let mut reader = Reader::new(cursor);
    for _ in 0..thread_number { // Skip ahead by `thread_number` values.
        reader.next().expect("Thread failed to skip ahead at start.");
    }
    println!("Starting search on thread {}/{}", thread_number, num_threads);
    let num_matches = grep_every_n(thread_number, num_threads, pattern, &mut reader)?;
    println!("Thread {}/{} is done. Found: {}", thread_number, num_threads, num_matches);
    Ok(num_matches)
}

fn grep_every_n<R: IonDataSource, C: Cursor<R>>(
    thread_num: usize,
    search_every_n: usize,
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

    let mut top_level_value_count = 0;
    let number_to_skip = search_every_n - 1;
    let mut match_count: usize = 0;
    'top_level: loop {
        // if thread_num == 0 && reader.depth() == 0 {
        //     let mut x = 5;
        //     x += number_to_skip;
        // }
        if reader.depth() == 0 {
            if top_level_value_count % search_every_n != 0 {
                // Skip the next (search_every_n - 1) values because other threads will be handling those.
                // println!("Skipping ahead by {}", (search_every_n - 1));
                for _ in 0..number_to_skip {
                    if reader.next()?.is_none() {
                        break 'top_level;
                    }
                }
                top_level_value_count += number_to_skip;
                continue;
            }
            top_level_value_count += 1;
        }

        let ion_type = match reader.next()? {
            Some((_, true)) => continue,
            Some((ion_type, false)) => ion_type,
            None if reader.depth() > 0 => {
                reader.step_out()?;
                continue;
            }
            None => {
                // We've run out of values at the top level, so we're done.
                break;
            },
        };

        // TODO: Only borrow once we need to
        let cache = symbol_match_cache.borrow();

        // Test the field ID if present
        if cache.len() > 0 {
            let field_name_matches = reader
                .field_id()
                .map(|sid| cache.contains(&sid))
                .unwrap_or(false);
            if field_name_matches {
                match_count += 1;
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
                    match_count += 1;
                    item_matched = true;
                }
            },
            Symbol => {
                if cache.len() > 0 {
                    let sid = reader.read_symbol_id()?.unwrap();
                    if cache.contains(&sid) {
                        match_count += 1;
                        item_matched = true;
                    }
                }
            },
            _ => {}
        }

        drop(cache);

        // TODO: This needs to happen at the top of the loop
        if reader.depth() > 0 {
            if item_matched {
                // This item matched, so we don't need to search the rest of it.
                // Call step_out() until we've reached the top level.
                reader.step_out()?;
                while reader.depth() > 0 {
                    reader.step_out()?;
                }
            }
        }
    }
    Ok(match_count)
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
