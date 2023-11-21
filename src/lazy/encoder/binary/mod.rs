use crate::lazy::encoder::binary::value_writer::{
    BinaryAnnotatedValueWriter_1_0, BinaryTopLevelValueWriter_1_0, BinaryValueWriter_1_0,
};
use crate::lazy::encoder::write_as_ion::WriteAsIon;
use crate::lazy::encoder::{LazyEncoder, LazyRawWriter, SequenceWriter, StructWriter, ValueWriter};
use crate::lazy::encoding::BinaryEncoding_1_0;
use crate::raw_symbol_token_ref::AsRawSymbolTokenRef;
use crate::IonResult;
use std::io::Write;
use std::marker::PhantomData;

use bumpalo::collections::Vec as BumpVec;
use bumpalo::Bump as BumpAllocator;
use delegate::delegate;

mod value_writer;

impl<W: Write> LazyEncoder<W> for BinaryEncoding_1_0 {
    type Writer = LazyRawBinaryWriter_1_0<W>;
    // type ValueWriter<'a> = BinaryValueWriter_1_0<'a>
    // where
    //     W: 'a;
    // type AnnotatedValueWriter<'a> = BinaryAnnotatedValueWriter_1_0<'a>
    // where
    //     W: 'a;
    type ListWriter<'a> = BinaryListWriter_1_0<'a>
    where
        W: 'a;
    type SExpWriter<'a> = BinarySExpWriter_1_0<'a>
    where
        W: 'a;
    type StructWriter<'a> = BinaryStructWriter_1_0<'a>
    where
        W: 'a;
    type EExpressionWriter<'a> = ();
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
        let top_level: &'_ mut BumpVec<'_, u8> = self
            .allocator
            .alloc_with(|| BumpVec::new_in(&self.allocator));
        let mut value_writer = BinaryValueWriter_1_0::new(0, top_level); //&self.allocator);
        let annotated_value_writer = BinaryTopLevelValueWriter_1_0::new(&mut value_writer);
        value.write_as_ion::<W, BinaryEncoding_1_0, BinaryTopLevelValueWriter_1_0>(
            annotated_value_writer,
        )?;

        let bytes = value_writer.buffer();
        self.output.write_all(bytes)?;
        Ok(self)
    }

    fn flush(&mut self) -> IonResult<()> {
        self.output.flush()?;
        Ok(())
    }
}

impl<'a, W: Write> LazyRawWriter<W, BinaryEncoding_1_0> for LazyRawBinaryWriter_1_0<W> {
    fn new(output: W) -> IonResult<Self> {
        Self::new(output)
    }

    delegate! {
        to self {
            fn write<V: WriteAsIon>(&mut self, _value: V) -> IonResult<&mut Self>;
            fn flush(&mut self) -> IonResult<()>;
        }
    }
}

pub struct BinaryListWriter_1_0<'a> {
    top_level_value_writer: &
}

impl<'a, W: Write> SequenceWriter<'a, W, BinaryEncoding_1_0> for BinaryListWriter_1_0<'a> {
    fn write<V: WriteAsIon>(&mut self, _value: V) -> IonResult<&mut Self> {
        todo!()
    }

    fn end(self) -> IonResult<()> {
        todo!()
    }
}

pub struct BinarySExpWriter_1_0<'a> {
    spooky: PhantomData<&'a ()>,
}

impl<'a, W: Write> SequenceWriter<'a, W, BinaryEncoding_1_0> for BinarySExpWriter_1_0<'a> {
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

impl<'a, W: Write> StructWriter<'a, W, BinaryEncoding_1_0> for BinaryStructWriter_1_0<'a> {
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
