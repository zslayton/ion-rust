extern crate ion_rust;

use ion_rust::result::IonResult;
use ion_rust::{BinaryIonCursor, Cursor, IonDataSource, IonType, Reader};
use std::fs::File;
use std::process::exit;

use regex::bytes::Regex;
use memmap::MmapOptions;
use std::io;

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

    let mut number_of_matches: usize = 0;
    if use_mmap {
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        let bytes_cursor = io::Cursor::new(mmap);
        let cursor = BinaryIonCursor::new(bytes_cursor);
        let mut reader = Reader::new(cursor);
        number_of_matches = grep(&pattern, &mut reader)?;
    } else {
        let buf_reader = std::io::BufReader::new(file);
        let cursor = BinaryIonCursor::new(buf_reader);
        let mut reader = Reader::new(cursor);
        number_of_matches = grep(&pattern, &mut reader)?;
    }
    println!("Found {} matches", number_of_matches);
    Ok(())
}

fn grep<R: IonDataSource, C: Cursor<R>>(
    pattern: &Regex,
    reader: &mut Reader<R, C>,
) -> IonResult<usize> {
    use IonType::*;
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

        let field_name_matches = reader
            .field_name()
            .map(|s| pattern.is_match(s.as_bytes()))
            .unwrap_or(false);
        if field_name_matches {
            count += 1;
            reader.step_out()?;
            continue;
        }
        let mut item_matched = false;
        match ion_type {
            Struct | List | SExpression => reader.step_in()?,
            String => {
                let matches = reader
                    .string_bytes_map(|s| pattern.is_match(s))?
                    .unwrap_or(false);
                if matches {
                    count += 1;
                    item_matched = true;
                }
            }
            Symbol => {
                let sid = reader.read_symbol_id()?.unwrap();
                let matches = reader
                    .symbol_table()
                    .text_for(sid)
                    .map(|text| pattern.is_match(text.as_bytes()))
                    .unwrap_or(false);
                if matches {
                    count += 1;
                    item_matched = true;
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
