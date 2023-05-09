use crate::data_source::IonDataSource;
use crate::result::{decoding_error, IonResult};
use std::io::Write;
use std::mem;

// ion_rust does not currently support reading variable length integers of truly arbitrary size.
// These type aliases will simplify the process of changing the data types used to represent each
// VarUInt's magnitude and byte length in the future.
// See: https://github.com/amazon-ion/ion-rust/issues/7

const BITS_PER_ENCODED_BYTE: usize = 7;
const STORAGE_SIZE_IN_BITS: usize = mem::size_of::<usize>() * 8;
// The following is ceiling division without requiring a conversion to f64.
// The expression is equivalent to: ceil(STORAGE_SIZE_IN_BITS / BITS_PER_ENCODED_BYTE)
const MAX_ENCODED_SIZE_IN_BYTES: usize =
    (STORAGE_SIZE_IN_BITS + BITS_PER_ENCODED_BYTE - 1) / BITS_PER_ENCODED_BYTE;

const LOWER_7_BITMASK: u8 = 0b0111_1111;
const HIGHEST_BIT_VALUE: u8 = 0b1000_0000;

/// Represents a variable-length unsigned integer. See the
/// [VarUInt and VarInt Fields](https://amazon-ion.github.io/ion-docs/docs/binary.html#varuint-and-varint-fields)
/// section of the binary Ion spec for more details.
#[derive(Debug)]
pub struct VarUInt {
    value: usize,
    size_in_bytes: usize,
}

impl VarUInt {
    pub(crate) fn new(value: usize, size_in_bytes: usize) -> Self {
        VarUInt {
            value,
            size_in_bytes,
        }
    }

    /// Reads a VarUInt from the provided data source.
    pub fn read<R: IonDataSource>(data_source: &mut R) -> IonResult<VarUInt> {
        let mut magnitude: usize = 0;
        let mut is_first_byte = true;
        let mut overflow_risk = false;

        let mut byte_processor = |byte: u8| {
            overflow_risk |= is_first_byte && byte > 1u8;
            is_first_byte = false;
            let lower_seven = (LOWER_7_BITMASK & byte) as usize;
            magnitude <<= 7; // Shifts 0 to 0 in the first iteration
            magnitude |= lower_seven;
            byte < HIGHEST_BIT_VALUE // If the highest bit is zero, continue reading
        };

        let encoded_size_in_bytes = data_source.read_next_byte_while(&mut byte_processor)?;

        // Prevent overflow by checking that the VarUInt was not too large to safely fit in the
        // data type being used to house the decoded value.
        //
        // This will return an error for inefficiently-encoded small values that use more bytes
        // than required. (e.g. A 10-byte encoding of the number 0 will be rejected.)
        //
        // However, reading VarUInt values is a very hot code path for reading binary Ion. This
        // compromise allows us to prevent overflows for the cost of a single branch per VarUInt
        // rather than performing extra bookkeeping logic on a per-byte basis.
        //
        // If the encoded value was more than 10 bytes long...
        if encoded_size_in_bytes > MAX_ENCODED_SIZE_IN_BYTES
            // or it was exactly 10 bytes, but the first byte contained more than a single bit.
            || encoded_size_in_bytes == MAX_ENCODED_SIZE_IN_BYTES && overflow_risk
        {
            return decoding_error(format!(
                "Found a {encoded_size_in_bytes}-byte VarUInt. Max supported size is {MAX_ENCODED_SIZE_IN_BYTES} bytes."
            ));
        }

        Ok(VarUInt {
            size_in_bytes: encoded_size_in_bytes,
            value: magnitude,
        })
    }

    /// Encodes the given unsigned int value as a VarUInt and writes it to the
    /// sink, returning the number of bytes written.
    pub fn write_u64<W: Write>(sink: &mut W, mut magnitude: u64) -> IonResult<usize> {
        // A u64 is 8 bytes of data. The VarUInt encoding will add a continuation bit to every byte,
        // plus one more termination bit. Therefore, the largest encoded size of a u64 is
        // 10 bytes; the last byte only uses one bit.
        const VAR_UINT_BUFFER_SIZE: usize = 10;

        // Create a buffer to store the encoded value.
        #[rustfmt::skip]
        let mut buffer: [u8; VAR_UINT_BUFFER_SIZE] = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0b1000_0000
            //                           ^-- Set the 'end' flag of the final byte to 1
        ];

        if magnitude == 0 {
            sink.write_all(&[0b1000_0000])?;
            return Ok(1);
        }

        // The encoding process moves right-to-left, from the last byte in the buffer to the first.
        // `first_byte` tracks the leftmost byte in the buffer that contains encoded data.
        // We will always write at least one byte.
        let mut first_byte = VAR_UINT_BUFFER_SIZE as u64;
        for buffer_byte in buffer.iter_mut().rev() {
            first_byte -= 1;
            *buffer_byte |= magnitude as u8 & LOWER_7_BITMASK;
            magnitude >>= BITS_PER_ENCODED_BYTE;
            if magnitude == 0 {
                break;
            }
        }

        let encoded_bytes = &buffer[(first_byte as usize)..];
        sink.write_all(encoded_bytes)?;
        Ok(encoded_bytes.len())
    }

    /// Returns the magnitude of the unsigned integer
    #[inline(always)]
    pub fn value(&self) -> usize {
        self.value
    }

    /// Returns the number of bytes that were read from the data source to construct this
    /// unsigned integer
    #[inline(always)]
    pub fn size_in_bytes(&self) -> usize {
        self.size_in_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::VarUInt;
    use crate::result::IonResult;
    use std::io::{BufReader, Cursor};

    const ERROR_MESSAGE: &str = "Failed to read a VarUInt from the provided data.";

    #[test]
    fn test_read_var_uint() {
        let var_uint = VarUInt::read(&mut Cursor::new(&[0b0111_1001, 0b0000_1111, 0b1000_0001]))
            .expect(ERROR_MESSAGE);
        assert_eq!(3, var_uint.size_in_bytes());
        assert_eq!(1_984_385, var_uint.value());
    }

    #[test]
    fn test_read_var_uint_small_buffer() {
        let var_uint = VarUInt::read(
            // Construct a BufReader whose input buffer cannot hold all of the data at once
            // to ensure that reads that span multiple I/O operations work as expected
            &mut BufReader::with_capacity(1, Cursor::new(&[0b0111_1001, 0b0000_1111, 0b1000_0001])),
        )
        .expect(ERROR_MESSAGE);
        assert_eq!(var_uint.size_in_bytes(), 3);
        assert_eq!(var_uint.value(), 1_984_385);
    }

    #[test]
    fn test_read_var_uint_zero() {
        let var_uint = VarUInt::read(&mut Cursor::new(&[0b1000_0000])).expect(ERROR_MESSAGE);
        assert_eq!(var_uint.size_in_bytes(), 1);
        assert_eq!(var_uint.value(), 0);
    }

    #[test]
    fn test_read_var_uint_two_bytes_max_value() {
        let var_uint =
            VarUInt::read(&mut Cursor::new(&[0b0111_1111, 0b1111_1111])).expect(ERROR_MESSAGE);
        assert_eq!(var_uint.size_in_bytes(), 2);
        assert_eq!(var_uint.value(), 16_383);
    }

    #[test]
    fn test_read_var_uint_overflow_detection() {
        let _var_uint = VarUInt::read(&mut Cursor::new(&[
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b1111_1111, // 1 byte more than the maximum allowed on 64-bit systems
        ]))
        .expect_err("This should have failed due to overflow.");
    }

    #[test]
    fn test_read_var_uint_slight_overflow_detection() {
        let _var_uint = VarUInt::read(&mut Cursor::new(&[
            0b0000_0011, // One bit more than the maximum allowed on 64-bit systems
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b0111_1111,
            0b1111_1111,
        ]))
        .expect_err("This should have failed due to overflow.");
    }

    fn var_uint_encoding_test(value: u64, expected_encoding: &[u8]) -> IonResult<()> {
        let mut buffer = vec![];
        VarUInt::write_u64(&mut buffer, value)?;
        assert_eq!(buffer.as_slice(), expected_encoding);
        Ok(())
    }

    #[test]
    fn test_write_var_uint_zero() -> IonResult<()> {
        var_uint_encoding_test(0, &[0b1000_0000])?;
        Ok(())
    }

    #[test]
    fn test_write_var_uint_single_byte_values() -> IonResult<()> {
        var_uint_encoding_test(6, &[0b1000_0110])?;
        var_uint_encoding_test(17, &[0b1001_0001])?;
        var_uint_encoding_test(41, &[0b1010_1001])?;
        Ok(())
    }

    #[test]
    fn test_write_var_uint_two_byte_values() -> IonResult<()> {
        var_uint_encoding_test(279, &[0b0000_0010, 0b1001_0111])?;
        var_uint_encoding_test(555, &[0b0000_0100, 0b1010_1011])?;
        var_uint_encoding_test(999, &[0b0000_0111, 0b1110_0111])?;
        Ok(())
    }

    #[test]
    fn test_write_var_uint_three_byte_values() -> IonResult<()> {
        var_uint_encoding_test(81_991, &[0b0000_0101, 0b0000_0000, 0b1100_0111])?;
        var_uint_encoding_test(400_600, &[0b0001_1000, 0b0011_1001, 0b1101_1000])?;
        Ok(())
    }

    #[test]
    fn roundtrip_u64_max() -> IonResult<()> {
        let mut buffer = Vec::new();
        let encoded_size = VarUInt::write_u64(&mut buffer, u64::MAX)?;
        assert_eq!(encoded_size, 10);
        println!("{:X?}", buffer);
        let value = VarUInt::read(&mut buffer.as_slice())?;
        assert_eq!(value.value() as u64, u64::MAX);
        Ok(())
    }
}
