use crate::lazy::encoder::binary::v1_1::container_writers::{
    BinaryContainerWriter_1_1, BinaryListWriter_1_1,
};
use crate::lazy::encoder::value_writer::SequenceWriter;
use crate::IonResult;

pub trait ContainerFn<W> {
    fn populate(self, writer: &mut W) -> IonResult<()>;
}

struct Quux;

impl<'value, 'top> ContainerFn<BinaryListWriter_1_1<'value, 'top>> for Quux {
    fn populate(self, sequence_writer: &mut BinaryListWriter_1_1<'value, 'top>) -> IonResult<()> {
        sequence_writer.write(1)?.write(2)?.write(3)?;
        Ok(())
    }
}

impl<F, W> ContainerFn<W> for F
where
    F: FnOnce(&mut W) -> IonResult<()>,
{
    fn populate(self, writer: &mut W) -> IonResult<()> {
        self(writer)
    }
}

mod tests {
    use super::*;

    #[test]
    fn try_it_out() -> IonResult<()> {
        let quux = Quux;
        let bump = bumpalo::Bump::new();
        let mut buffer = bumpalo::collections::Vec::new_in(&bump);
        let cw = BinaryContainerWriter_1_1::new(&bump, &mut buffer);
        let mut lw = BinaryListWriter_1_1::new(cw);
        quux.populate(&mut lw)?;
        println!("{} bytes", lw.container_writer.buffer().len());
        Ok(())
    }

    #[test]
    fn try_it_out_for_real() -> IonResult<()> {
        let quux = |list: &mut BinaryListWriter_1_1| {
            list.write(1)?.write(2)?.write(3)?;
            Ok(())
        };
        let bump = bumpalo::Bump::new();
        let mut buffer = bumpalo::collections::Vec::new_in(&bump);
        let cw = BinaryContainerWriter_1_1::new(&bump, &mut buffer);
        let mut lw = BinaryListWriter_1_1::new(cw);
        quux.populate(&mut lw)?;
        println!("{} bytes", lw.container_writer.buffer().len());
        Ok(())
    }
}
