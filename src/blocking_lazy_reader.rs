use crate::lazy::any_encoding::AnyEncoding;
use crate::lazy::binary::raw::reader::LazyRawBinaryReader;
use crate::lazy::decoder::{LazyDecoder, LazyRawReader};
use crate::lazy::raw_stream_item::LazyRawStreamItem;
use crate::{IonError, IonResult};
use bumpalo::Bump as BumpAllocator;
use std::marker::PhantomData;

/// The BlockingLazyRawReader wraps a non-blocking LazyRawReader that implements the LazyRawReader trait,
/// providing a blocking LazyRawReader.
pub struct BlockingLazyRawReader<'data, D: LazyDecoder, R: LazyRawReader<'data, D>> {
    source: &'data [u8],
    buffer: Vec<u8>,
    reader: R,
    expected_read_size: usize,
    allocator: BumpAllocator,
    offset: usize,
    phantom: PhantomData<D>,
}

const READER_DEFAULT_BUFFER_CAPACITY: usize = 1024 * 4;
impl<'data, D: LazyDecoder, R: LazyRawReader<'data, D>> BlockingLazyRawReader<'data, D, R> {
    pub fn read_source(&mut self, length: usize) -> IonResult<usize> {
        let mut bytes_read = 0;
        loop {
            let n = self.reader.read_from(&mut self.source, length)?;
            bytes_read += n;
            if n == 0 || bytes_read >= length {
                break;
            }
        }
        Ok(bytes_read)
    }

    pub fn new(input: &'data [u8]) -> IonResult<Self> {
        Self::new_with_size(input, READER_DEFAULT_BUFFER_CAPACITY)
    }

    pub fn new_with_size(source: &'data [u8], size: usize) -> IonResult<Self> {
        let buffer = Vec::with_capacity(size);
        let mut reader = Self {
            source,
            buffer: buffer.clone(),
            reader: LazyRawReader::new(buffer.as_slice()),
            expected_read_size: size,
            allocator: Default::default(),
            offset: 0,
            phantom: Default::default(),
        };
        reader.read_source(size)?;
        Ok(reader)
    }

    fn next<'top>(&'top mut self) -> IonResult<LazyRawStreamItem<'top, D>>
    where
        'data: 'top,
    {
        let mut nb_reader = R::new_with_offset(&self.buffer[self.offset..], self.offset);
        let result = nb_reader.next(&self.allocator);
        return match result {
            Ok(item) => {
                self.offset = nb_reader.next_item_offset();
                Ok(item)
            }
            Err(IonError::Incomplete { .. }) => {
                let mut read_size = self.expected_read_size;
                // Refill buffer, update offset and retry in a loop
                loop {
                    let bytes_read = self.read_source(read_size)?;
                    // if we have no bytes, and our stream has been marked as fully loaded, then we
                    // need to bubble up the error. Otherwise, if our stream has not been marked as
                    // loaded, then we need to mark it as loaded and retry.
                    if 0 == bytes_read {
                        if self.reader.is_stream_complete() {
                            return result;
                        } else {
                            self.reader.stream_complete();
                        }
                    }
                    // The assumption here is that most buffer sizes will start at a magnitude the user
                    // is comfortable with in terms of memory usage. So if we're reading more in order
                    // to reach a parsable point we do not want to start consuming more than an order of
                    // magnitude more memory just to get there.
                    read_size = std::cmp::min(read_size * 2, self.expected_read_size * 10);
                }
            }
            Err(e) => Err(e),
        };
    }
}
