use crate::lazy::encoder::binary::value_writer::{
    BinaryAnnotatedValueWriter_1_0, BinaryValueWriter_1_0, MAX_INLINE_LENGTH,
};
use crate::lazy::encoder::write_as_ion::WriteAsIon;
use crate::lazy::encoder::{LazyEncoder, LazyRawWriter, SequenceWriter, StructWriter};
use crate::lazy::encoding::BinaryEncoding_1_0;
use crate::raw_symbol_token_ref::AsRawSymbolTokenRef;
use crate::{IonError, IonResult, RawSymbolTokenRef};
use std::io::Write;

use crate::binary::var_uint::VarUInt;
use crate::result::EncodingError;
use bumpalo::collections::Vec as BumpVec;
use bumpalo::Bump as BumpAllocator;
use delegate::delegate;

mod value_writer;

impl<W: Write> LazyEncoder<W> for BinaryEncoding_1_0 {
    type Writer = LazyRawBinaryWriter_1_0<W>;
}

/// A "raw"-level streaming binary Ion writer. This writer does not provide symbol table
/// management; symbol-related operations (e.g. setting field IDs and annotations or writing symbol
/// values) require a valid symbol ID to be provided by the caller.
pub struct LazyRawBinaryWriter_1_0<W: Write> {
    // The sink to which all of the writer's encoded data will be written.
    output: W,
    // A bump allocator that can be used to cheaply create scratch buffers for nested container
    // encoding.
    allocator: BumpAllocator,
    // A pointer to the bump-allocated top-level encoding buffer, if set.
    //
    // This buffer is constructed in `allocator` above, a region of memory over which we have
    // complete control. When the allocator creates a buffer, the buffer has a lifetime equivalent to
    // the lifetime of the function in which it was created. However, we know that the data it contains
    // will continue to be valid even after that method is complete and any return values are dropped.
    // Thus, we store a raw pointer to the buffer and use an `Option` to track whether the pointer
    // is set to a meaningful address. This allows us to refer to the contents of the buffer across
    // multiple mutable calls of `write` and `value_writer()`.
    encoding_buffer_ptr: Option<*mut ()>,
}

impl<W: Write> LazyRawBinaryWriter_1_0<W> {
    /// Constructs a new binary writer and writes an Ion 1.0 Version Marker to output.
    pub fn new(mut output: W) -> IonResult<Self> {
        // Write the Ion 1.0 IVM
        output.write_all(&[0xE0, 0x01, 0x00, 0xEA])?;
        // Construct the writer
        Ok(Self {
            output,
            allocator: BumpAllocator::new(),
            encoding_buffer_ptr: None,
        })
    }

    /// Helper function that turns a raw pointer into a mutable reference of the specified type.
    fn ptr_to_mut_ref<'a, T>(ptr: *mut ()) -> &'a mut T {
        let typed_ptr: *mut T = ptr.cast();
        unsafe { &mut *typed_ptr }
    }

    /// Helper function that turns a mutable reference into a raw pointer.
    fn mut_ref_to_ptr<T>(reference: &mut T) -> *mut () {
        let ptr: *mut T = reference;
        let untyped_ptr: *mut () = ptr.cast();
        untyped_ptr
    }

    /// Writes the given Rust value to the output stream as a top-level value.
    fn write<V: WriteAsIon>(&mut self, value: V) -> IonResult<&mut Self> {
        value.write_as_ion(self.value_writer())?;
        Ok(self)
    }

    /// Flushes any encoded bytes that have not already been written to the output sink.
    ///
    /// Calling `flush` also releases memory used for bookkeeping and storage, but calling it
    /// frequently can reduce overall throughput.
    fn flush(&mut self) -> IonResult<()> {
        // Temporarily break apart `self` to get simultaneous references to its innards.
        let Self {
            output,
            allocator,
            encoding_buffer_ptr,
        } = self;

        let encoding_buffer = match encoding_buffer_ptr {
            // If `encoding_buffer_ptr` is set, get the slice of bytes to which it refers.
            Some(ptr) => Self::ptr_to_mut_ref::<'_, BumpVec<'_, u8>>(*ptr).as_slice(),
            // Otherwise, there's nothing in the buffer. Use an empty slice.
            None => &[],
        };
        // Write our top level encoding buffer's contents to the output sink.
        output.write_all(encoding_buffer)?;
        // Flush the output sink, which may have its own buffers.
        output.flush()?;
        // Clear the allocator. A new encoding buffer will be allocated on the next write.
        allocator.reset();
        Ok(())
    }
}

impl<W: Write> LazyRawWriter<W> for LazyRawBinaryWriter_1_0<W> {
    // At the top level, the value writer's lifetimes are the same. They are both `'top`.
    type ValueWriter<'a> = BinaryAnnotatedValueWriter_1_0<'a, 'a>
    where
        Self: 'a;

    fn new(output: W) -> IonResult<Self> {
        Self::new(output)
    }

    fn value_writer(&mut self) -> Self::ValueWriter<'_> {
        let top_level = match self.encoding_buffer_ptr {
            // If the `encoding_buffer_ptr` is set, we already allocated an encoding buffer on
            // a previous call to `value_writer()`. Dereference the pointer and continue encoding
            // to that buffer.
            Some(ptr) => Self::ptr_to_mut_ref::<'_, BumpVec<'_, u8>>(ptr),
            // Otherwise, allocate a new encoding buffer and set the pointer to refer to it.
            None => {
                let buffer = self
                    .allocator
                    .alloc_with(|| BumpVec::new_in(&self.allocator));
                self.encoding_buffer_ptr = Some(Self::mut_ref_to_ptr(buffer));
                buffer
            }
        };
        let value_writer = BinaryValueWriter_1_0::new(&self.allocator, top_level);
        let annotated_value_writer = BinaryAnnotatedValueWriter_1_0::new(value_writer);
        annotated_value_writer
    }

    delegate! {
        to self {
            fn write<V: WriteAsIon>(&mut self, _value: V) -> IonResult<&mut Self>;
            fn flush(&mut self) -> IonResult<()>;
        }
    }
}

pub(crate) struct BinaryContainerWriter_1_0<'value, 'top> {
    // 0xB0 for list, 0xC0 for sexp, 0xD0 for struct
    // We could use an enum for this, but it would require branching at the end of every container.
    type_code: u8,
    // An allocator reference that can be shared with nested container writers
    allocator: &'top BumpAllocator,
    // The buffer containing the parent's encoded body. When this list writer is finished encoding
    // its own data, a header will be written to the parent and then the list body will be copied
    // over.
    parent_buffer: &'value mut BumpVec<'top, u8>,
    // The body of the list this writer is responsible for encoding.
    encoding_buffer: BumpVec<'top, u8>,
}

impl<'value, 'top> BinaryContainerWriter_1_0<'value, 'top> {
    pub fn new(
        type_code: u8,
        allocator: &'top BumpAllocator,
        parent_buffer: &'value mut BumpVec<'top, u8>,
    ) -> Self {
        let encoding_buffer = BumpVec::new_in(allocator);
        Self {
            type_code,
            allocator,
            parent_buffer,
            encoding_buffer,
        }
    }

    fn write<V: WriteAsIon>(&mut self, value: V) -> IonResult<&mut Self> {
        let value_writer = BinaryValueWriter_1_0::new(self.allocator, &mut self.encoding_buffer);
        let annotated_value_writer = BinaryAnnotatedValueWriter_1_0::new(value_writer);
        value.write_as_ion(annotated_value_writer)?;
        Ok(self)
    }

    fn end(mut self) -> IonResult<()> {
        let body_length = self.encoding_buffer.len();
        if body_length <= MAX_INLINE_LENGTH {
            let type_descriptor = self.type_code | (body_length as u8);
            self.parent_buffer.push(type_descriptor);
        } else {
            self.parent_buffer.push(self.type_code | 0xE);
            VarUInt::write_u64(&mut self.parent_buffer, body_length as u64)?;
        }
        self.parent_buffer
            .extend_from_slice(self.encoding_buffer.as_slice());
        Ok(())
    }
}

pub struct BinaryListWriter_1_0<'value, 'top> {
    sequence_writer: BinaryContainerWriter_1_0<'value, 'top>,
}

impl<'value, 'top> BinaryListWriter_1_0<'value, 'top> {
    pub(crate) fn new(sequence_writer: BinaryContainerWriter_1_0<'value, 'top>) -> Self {
        Self { sequence_writer }
    }
}

impl<'value, 'top> SequenceWriter for BinaryListWriter_1_0<'value, 'top> {
    fn write<V: WriteAsIon>(&mut self, value: V) -> IonResult<&mut Self> {
        self.sequence_writer.write(value)?;
        Ok(self)
    }

    delegate! {
        to self.sequence_writer {
            fn end(self) -> IonResult<()>;
        }
    }
}

pub struct BinarySExpWriter_1_0<'value, 'top> {
    sequence_writer: BinaryContainerWriter_1_0<'value, 'top>,
}

impl<'value, 'top> BinarySExpWriter_1_0<'value, 'top> {
    pub(crate) fn new(sequence_writer: BinaryContainerWriter_1_0<'value, 'top>) -> Self {
        Self { sequence_writer }
    }
}

impl<'value, 'top> SequenceWriter for BinarySExpWriter_1_0<'value, 'top> {
    fn write<V: WriteAsIon>(&mut self, value: V) -> IonResult<&mut Self> {
        self.sequence_writer.write(value)?;
        Ok(self)
    }
    delegate! {
        to self.sequence_writer {
            fn end(self) -> IonResult<()>;
        }
    }
}

pub struct BinaryStructWriter_1_0<'value, 'top> {
    container: BinaryContainerWriter_1_0<'value, 'top>,
}

impl<'value, 'top> BinaryStructWriter_1_0<'value, 'top> {
    pub(crate) fn new(container: BinaryContainerWriter_1_0<'value, 'top>) -> Self {
        Self { container }
    }
}

impl<'value, 'top> StructWriter for BinaryStructWriter_1_0<'value, 'top> {
    fn write<A: AsRawSymbolTokenRef, V: WriteAsIon>(
        &mut self,
        name: A,
        value: V,
    ) -> IonResult<&mut Self> {
        let sid = match name.as_raw_symbol_token_ref() {
            RawSymbolTokenRef::SymbolId(sid) => sid,
            RawSymbolTokenRef::Text(text) => {
                return Err(IonError::Encoding(EncodingError::new(format!(
                    "tried to write text literal using raw binary writer: '{text}'"
                ))));
            }
        };
        VarUInt::write_u64(&mut self.container.encoding_buffer, sid as u64)?;
        self.container.write(value)?;
        Ok(self)
    }

    delegate! {
        to self.container {
            fn end(self) -> IonResult<()>;
        }
    }
}
