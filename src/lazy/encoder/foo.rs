use crate::lazy::encoder::binary::v1_1::container_writers::{
    BinaryContainerWriter_1_1, BinaryListWriter_1_1,
};
use crate::lazy::encoder::value_writer::{SequenceWriter, ValueWriter};
use crate::IonResult;

///! Container population traits that allow closures to be used in places where the borrow checker
/// would normally balk due to point-in-time limitations. TODO link

// ===== List =====
pub trait ListFn<V: ValueWriter>: FnOnce(&mut V::ListWriter) -> IonResult<()> {
    fn populate(self, writer: &mut V::ListWriter) -> IonResult<()>;
}

impl<F, V: ValueWriter> ListFn<V> for F
    where
        F: FnOnce(&mut V::ListWriter) -> IonResult<()>,
{
    fn populate(self, writer: &mut V::ListWriter) -> IonResult<()> {
        self(writer)
    }
}

// ===== SExp =====

pub trait SExpFn<V: ValueWriter>: FnOnce(&mut V::SExpWriter) -> IonResult<()> {
    fn populate(self, writer: &mut V::SExpWriter) -> IonResult<()>;
}

impl<F, V: ValueWriter> SExpFn<V> for F
    where
        F: FnOnce(&mut V::SExpWriter) -> IonResult<()>,
{
    fn populate(self, writer: &mut V::SExpWriter) -> IonResult<()> {
        self(writer)
    }
}

trait Foo {
    fn foo(self) -> Result<(), ()>;
}

struct Bar;
impl Foo for &mut Bar {
    fn foo(self) -> Result<(), ()> {
        println!("works");
        Ok(())
    }
}

#[test]
fn gogogo() -> Result<(), ()>{
    let mut bar = Bar;
    bar.foo()?;
    bar.foo()
}

// struct Quux;
//
// impl<'value, 'top> ContainerFn<BinaryListWriter_1_1<'value, 'top>> for Quux {
//     fn populate(self, sequence_writer: &mut BinaryListWriter_1_1<'value, 'top>) -> IonResult<()> {
//         sequence_writer.write(1)?.write(2)?.write(3)?;
//         Ok(())
//     }
// }


mod tests {
    use crate::lazy::encoder::binary::v1_0::writer::LazyRawBinaryWriter_1_0;
    use super::*;

    use crate::lazy::encoder::value_writer::{SequenceWriter, ValueWriter};

    // #[test]
    // fn try_it_out() -> IonResult<()> {
    //     let quux = Quux;
    //     let bump = bumpalo::Bump::new();
    //     let mut buffer = bumpalo::collections::Vec::new_in(&bump);
    //     let cw = BinaryContainerWriter_1_1::new(&bump, &mut buffer);
    //     let mut lw = BinaryListWriter_1_1::new(cw);
    //     quux.populate(&mut lw)?;
    //     println!("{} bytes", lw.container_writer.buffer().len());
    //     Ok(())
    // }

    // #[test]
    // fn try_it_out_for_real() -> IonResult<()> {
    //     let quux = |list| {
    //         list.write(1)?.write(2)?.write(3)?;
    //         Ok(())
    //     };
    //     let bump = bumpalo::Bump::new();
    //     let mut buffer = bumpalo::collections::Vec::new_in(&bump);
    //     let cw = BinaryContainerWriter_1_1::new(&bump, &mut buffer);
    //     let mut lw = BinaryListWriter_1_1::new(cw);
    //     quux.populate(&mut lw)?;
    //     println!("{} bytes", lw.container_writer.buffer().len());
    //     Ok(())
    // }

    // #[test]
    // fn try_it_out_for_real() -> IonResult<()> {
    //     let quux = |list| {
    //         list.write(1)?.write(2)?.write(3)?;
    //         Ok(())
    //     };
    //     let bump = bumpalo::Bump::new();
    //     let mut buffer = bumpalo::collections::Vec::new_in(&bump);
    //     let cw = BinaryContainerWriter_1_1::new(&bump, &mut buffer);
    //     let mut lw = BinaryListWriter_1_1::new(cw);
    //     quux.populate(&mut lw)?;
    //
    //     let mut output = Vec::new();
    //     let mut writer = LazyRawBinaryWriter_1_0::new(output)?;
    //     writer.value_writer().write_list(quux)?;
    //     println!("{} bytes", lw.container_writer.buffer().len());
    //     Ok(())
    // }

    #[test]
    fn try_it_out_for_real() -> IonResult<()> {
        let mut output = Vec::new();
        let mut writer = LazyRawBinaryWriter_1_0::new(&mut output)?;
        writer.value_writer().write_list(|list| {
            list.write(1)?.write(2)?.write(3)?;
            Ok(())
        })?;
        writer.flush()?;
        println!("{} bytes", output.len());
        Ok(())
    }
}
