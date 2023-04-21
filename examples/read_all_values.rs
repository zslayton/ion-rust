use ion_rs::binary::non_blocking::raw_binary_reader::RawBinaryReader;
use ion_rs::raw_reader::RawStreamItem;
use ion_rs::result::IonResult;
use ion_rs::types::value_ref::RawValueRef;
use ion_rs::BlockingRawBinaryReader;
use ion_rs::RawIonReader;
use memmap::MmapOptions;
use std::fs::File;
use std::process::exit;

fn main() -> IonResult<()> {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).unwrap_or_else(|| {
        eprintln!(
            "USAGE:\n\n    {} [blocking|nonblocking] [Binary Ion file]\n",
            args.get(0).unwrap()
        );
        eprintln!("No mode was specified.");
        exit(1);
    });
    let path = args.get(2).unwrap_or_else(|| {
        eprintln!(
            "USAGE:\n\n    {} [blocking|nonblocking] [Binary Ion file]\n",
            args.get(0).unwrap()
        );
        eprintln!("No input file was specified.");
        exit(2);
    });
    let file = File::open(path).unwrap();

    // This example uses `mmap` so we can use either the blocking reader (which reads from an
    // io::BufRead) or the non-blocking reader (which reads from an AsRef<[u8]>).
    let mmap = unsafe { MmapOptions::new().map(&file).unwrap() };

    // Treat the mmap as a byte array.
    let ion_data: &[u8] = &mmap[..];

    if mode == "blocking" {
        let mut reader = BlockingRawBinaryReader::new(ion_data)?;
        let number_of_values = read_all_values(&mut reader)?;
        println!("Blocking: read {} values", number_of_values);
    } else if mode == "nonblocking" {
        let mut reader = RawBinaryReader::new(ion_data);
        let number_of_values = read_all_values(&mut reader)?;
        println!("Non-blocking: read {} values", number_of_values);
    } else {
        eprintln!("Unsupported `mode`: {}.", mode);
        exit(3);
    }

    Ok(())
}

// Visits each value in the stream recursively, reading each scalar into a native Rust type.
// Prints the total number of values read upon completion.
fn read_all_values<R: RawIonReader>(reader: &mut R) -> IonResult<usize> {
    use RawStreamItem::{Nothing, Null as NullValue, Value, VersionMarker};
    let mut count: usize = 0;
    loop {
        match reader.next()? {
            VersionMarker(_major, _minor) => {}
            NullValue(_) | Value(_) => {
                count += 1;
                match reader.read_value()? {
                    RawValueRef::SExp | RawValueRef::List | RawValueRef::Struct => {
                        reader.step_in()?;
                    }
                    _ => {}
                }
                continue;
            }
            Nothing if reader.depth() > 0 => {
                reader.step_out()?;
            }
            Nothing => {
                break;
            }
        }
    }
    Ok(count)
}
