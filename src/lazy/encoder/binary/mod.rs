use crate::lazy::encoder::binary::value_writer::{
    BinaryAnnotatedValueWriter_1_0, BinaryValueWriter_1_0, MAX_INLINE_LENGTH,
};
use crate::lazy::encoder::write_as_ion::WriteAsIon;
use crate::lazy::encoder::{LazyEncoder, LazyRawWriter, SequenceWriter, StructWriter};
use crate::lazy::encoding::BinaryEncoding_1_0;
use crate::raw_symbol_token_ref::AsRawSymbolTokenRef;
use crate::IonResult;
use std::io::Write;
use std::marker::PhantomData;

use crate::binary::var_uint::VarUInt;
use bumpalo::collections::Vec as BumpVec;
use bumpalo::Bump as BumpAllocator;
use delegate::delegate;

mod value_writer;

impl<W: Write> LazyEncoder<W> for BinaryEncoding_1_0 {
    type Writer = LazyRawBinaryWriter_1_0<W>;
}

pub struct LazyRawBinaryWriter_1_0<W: Write> {
    output: W,
    allocator: BumpAllocator,
}

impl<W: Write> LazyRawBinaryWriter_1_0<W> {
    pub fn new(mut output: W) -> IonResult<Self> {
        output.write_all(&[0xE0, 0x01, 0x00, 0xEA])?;
        Ok(Self {
            output,
            allocator: BumpAllocator::new(),
        })
    }

    fn write<V: WriteAsIon>(&mut self, value: V) -> IonResult<&mut Self> {
        self.allocator.reset();
        let mut top_level = BumpVec::new_in(&self.allocator);
        let value_writer = BinaryValueWriter_1_0::new(&self.allocator, &mut top_level);
        let annotated_value_writer = BinaryAnnotatedValueWriter_1_0::new(value_writer);
        value.write_as_ion::<BinaryAnnotatedValueWriter_1_0>(annotated_value_writer)?;
        self.output.write_all(top_level.as_slice())?;
        drop(top_level);
        Ok(self)
    }

    fn flush(&mut self) -> IonResult<()> {
        self.output.flush()?;
        Ok(())
    }
}

impl<W: Write> LazyRawWriter<W> for LazyRawBinaryWriter_1_0<W> {
    type ValueWriter<'a> = BinaryValueWriter_1_0<'a, 'a>
    where
        Self: 'a;

    fn new(output: W) -> IonResult<Self> {
        Self::new(output)
    }

    fn value_writer<'a>(&'a mut self) -> Self::ValueWriter<'a> {
        todo!()
    }

    delegate! {
        to self {
            fn write<V: WriteAsIon>(&mut self, _value: V) -> IonResult<&mut Self>;
            fn flush(&mut self) -> IonResult<()>;
        }
    }
}

pub struct BinaryListWriter_1_0<'value, 'top> {
    // An allocator reference that can be shared with nested container writers
    allocator: &'top BumpAllocator,
    // The buffer containing the parent's encoded body. When this list writer is finished encoding
    // its own data, a header will be written to the parent and then the list body will be copied
    // over.
    parent_buffer: &'value mut BumpVec<'top, u8>,
    // The body of the list this writer is responsible for encoding.
    encoding_buffer: BumpVec<'top, u8>,
}

impl<'value, 'top> BinaryListWriter_1_0<'value, 'top> {
    pub fn new(
        allocator: &'top BumpAllocator,
        parent_buffer: &'value mut BumpVec<'top, u8>,
    ) -> Self {
        let encoding_buffer = BumpVec::new_in(allocator);
        Self {
            allocator,
            parent_buffer,
            encoding_buffer,
        }
    }
}

impl<'value, 'top> SequenceWriter for BinaryListWriter_1_0<'value, 'top> {
    fn write<V: WriteAsIon>(&mut self, value: V) -> IonResult<&mut Self> {
        let value_writer = BinaryValueWriter_1_0::new(self.allocator, &mut self.encoding_buffer);
        let annotated_value_writer = BinaryAnnotatedValueWriter_1_0::new(value_writer);
        value.write_as_ion(annotated_value_writer)?;
        Ok(self)
    }

    fn end(mut self) -> IonResult<()> {
        let body_length = self.encoding_buffer.len();
        if body_length <= MAX_INLINE_LENGTH {
            let type_descriptor = 0xB0 | (body_length as u8);
            self.parent_buffer.push(type_descriptor);
        } else {
            self.parent_buffer.push(0xBE);
            VarUInt::write_u64(&mut self.parent_buffer, body_length as u64)?;
        }
        self.parent_buffer
            .extend_from_slice(self.encoding_buffer.as_slice());
        Ok(())
    }
}

pub struct BinarySExpWriter_1_0<'a> {
    spooky: PhantomData<&'a ()>,
}

impl<'a> SequenceWriter for BinarySExpWriter_1_0<'a> {
    fn write<V: WriteAsIon>(&mut self, _value: V) -> IonResult<&mut Self> {
        todo!()
    }

    fn end(self) -> IonResult<()> {
        todo!()
    }
}

pub struct BinaryStructWriter_1_0<'a> {
    spooky: PhantomData<&'a ()>,
}

impl<'a> StructWriter for BinaryStructWriter_1_0<'a> {
    fn write<A: AsRawSymbolTokenRef, V: WriteAsIon>(
        &mut self,
        _name: A,
        _value: V,
    ) -> IonResult<&mut Self> {
        todo!()
    }

    fn end(self) -> IonResult<()> {
        todo!()
    }
}
