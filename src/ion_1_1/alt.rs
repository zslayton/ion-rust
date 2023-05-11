use crate::result::decoding_error;
use crate::IonResult;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Write;

const BITS_PER_U64: u32 = 64;
const BITS_PER_ENCODED_BYTE: u32 = 7;

// 0 to 64 inclusive, 65 values
const fn init_bytes_needed_cache() -> [u8; 65] {
    let mut cache = [0u8; 65];
    let mut leading_zeros = 0usize;
    while leading_zeros < 64 {
        let magnitude_bits_needed = 64 - leading_zeros;
        cache[leading_zeros] = ((magnitude_bits_needed as u32 + BITS_PER_ENCODED_BYTE - 1)
            / BITS_PER_ENCODED_BYTE) as u8;
        leading_zeros += 1;
    }
    // Special case; 64 leading zeros means the value is zero. We need a byte to represent it anyway.
    cache[64] = 1;
    cache
}

#[test]
fn enc() {
    let mut buffer = Vec::new();
    encode_var_uint(&mut buffer, 21043).unwrap();
    println!("{:x?}", buffer);
    for byte in buffer {
        println!("{:08b}", byte);
    }
}

static BYTES_NEEDED_CACHE: [u8; 65] = init_bytes_needed_cache();

pub fn encode_var_uint<W: Write>(output: &mut W, value: u64) -> IonResult<usize> {
    if value < 0x80 {
        output.write_all(&[(value * 2) as u8 + 1])?;
        return Ok(1);
    } else if value < 0x4000 {
        output.write_all(&((value * 4) as u16 + 2u16).to_le_bytes())?;
        return Ok(2);
    }
    let leading_zeros = value.leading_zeros();
    // The following is ceiling division without requiring a conversion to f64.
    // The expression is equivalent to: ceil(magnitude_bits_needed / BITS_PER_ENCODED_BYTE)
    let num_encoded_bytes = BYTES_NEEDED_CACHE[leading_zeros as usize] as usize;

    match num_encoded_bytes {
        0..=8 => {
            // When encoded, the continuation flags and the value all fit in 8 bytes. We can encode
            // everything in a u64 and then write it to output.
            //
            // There's one continuation flag bit for each encoded byte. To set the bits:
            // * Left shift a `1` by the number of bytes minus one.
            //
            // For example, if `num_encoded_bytes` is 5, then:
            //   1 << 4   =>   1 0000
            //      End flag --^ ^^^^-- Four more bytes follow
            let flag_bits = 1u64 << (num_encoded_bytes - 1);
            // Left shift the value to accommodate the trailing flag bits and then OR them together
            let encoded_value = (value << num_encoded_bytes) | flag_bits;
            output.write_all(&encoded_value.to_le_bytes()[..num_encoded_bytes as usize])?;
            Ok(num_encoded_bytes)
        }
        9 => {
            // When combined with the continuation flags, the value is too large to be encoded in
            // a u64. It will be nine bytes in all.
            //
            // Set up a stack-allocated buffer to hold our encoding. This allows us to call
            // `output.write_all()` a single time.
            let mut buffer: [u8; 9] = [0; 9];

            // The first byte will always be 0x00, indicating that 8 more bytes follow.
            //
            // We need to leave a `1` in the low bit of the second byte to be the End flag. Because
            // we need fewer than 64 bits for magnitude, we can encode the remainder of the data
            // in a u64.
            let encoded_value = (value << 1) + 1; // Leave a trailing `1` in the lowest bit
            buffer[1..].copy_from_slice(&encoded_value.to_le_bytes()[..]);
            output.write_all(buffer.as_slice())?;
            Ok(9)
        }
        10 => {
            // Set up a stack-allocated buffer to hold our encoding. This allows us to call
            // `output.write_all()` a single time.
            let mut buffer: [u8; 10] = [0; 10];
            // The first byte in the encoding is always 0x00, indicating that at least 8 more bytes
            // follow. The second byte has two more continuation flag bits (`10`), indicating that
            // the whole value is 10 bytes long. We can fit 6 bits of magnitude in this second byte.
            let second_byte = ((value & 0b111111) << 2) as u8 | 0b10u8;
            buffer[1] = second_byte;

            // The remaining 58 bits of magnitude can be encoded in a u64.
            let remaining_magnitude: u64 = value >> 6;
            buffer[2..].copy_from_slice(&remaining_magnitude.to_le_bytes()[..]);

            // Call `write_all()` once with our complete encoding.
            output.write_all(buffer.as_slice()).unwrap();
            Ok(10)
        }
        _ => unreachable!("a u64 value cannot have more than 64 magnitude bits"),
    }
}

pub fn decode_var_uint(bytes: &[u8]) -> IonResult<(usize, u64)> {
    // A temporary, stack-allocated buffer
    let mut buffer = [0u8; 10];
    // If the input doesn't have at least 10 bytes, copy it into our temporary buffer to simplify
    // reads.
    let bytes = if bytes.len() >= 10 {
        bytes
    } else {
        buffer[0..bytes.len()].clone_from_slice(bytes);
        &buffer[..]
    };

    match (bytes[0], bytes[1]) {
        (0x00, b2) if b2 & 0b11 == 0b00 => {
            // The flag bits in the second byte indicate at least two more bytes, meaning the total
            // length is more than 10 bytes. We're not equipped to handle this.
            decoding_error("found a >10 byte VarUInt too large to fit in a u64")
        }
        (0x00, b2) if b2 & 0b11 == 0b10 => {
            // The lowest bit of the second byte is empty, the next lowest is not. The encoding
            // is 10 bytes; there are 64 bits of magnitude.
            let low_six = b2 >> 2;
            let mut remaining_data = &bytes[2..];
            let remaining_magnitude = remaining_data.read_u64::<LittleEndian>()?;
            // Make sure none of the highest 6 bits are set, because that would be overflow
            if remaining_magnitude > (1u64 << 58) - 1 {
                return decoding_error("found a 10-byte VarUInt too large to fit in a u64");
            }
            let value = (remaining_magnitude << 6) | low_six as u64;
            Ok((10, value))
        }
        (0x00, _) => {
            // The lowest bit of the second byte is not set. The encoding is 9 bytes. There are
            // 57-63 bits of magnitude. We can decode the remaining bytes in a u64.
            let mut remaining_data = &bytes[1..];
            // Lop off the lowest bit of the next 8 bytes.
            let value = remaining_data.read_u64::<LittleEndian>()? >> 1;
            Ok((9, value))
        }
        (b1, _) => {
            // The common case. There 7 bytes' worth of magnitude (or fewer) to decode.
            let num_encoded_bytes = b1.trailing_zeros() as usize + 1;
            let num_encoded_bits = 8 * num_encoded_bytes;
            // Get a mask with the low 'n' bits set
            let mask = 1u64
                .checked_shl(num_encoded_bits as u32)
                .map(|v| v - 1)
                .unwrap_or(u64::MAX);
            let encoded_value = bytes.clone().read_u64::<LittleEndian>()?;
            // Note that `num_encoded_bytes` is also the number of continuation flags to ignore
            let value = (encoded_value & mask) >> num_encoded_bytes;
            Ok((num_encoded_bytes, value))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::var_uint::VarUInt;
    use rand::prelude::*;

    pub fn generate_integers(int_size_in_bytes: usize, num_ints: usize) -> Vec<u64> {
        let mut rng = StdRng::seed_from_u64(1024);

        let mut data: Vec<u64> = Vec::with_capacity(num_ints);

        let num_bits_per_int = int_size_in_bytes * 8;
        let shifted = 1u64.checked_shl(num_bits_per_int as u32).unwrap_or(0);
        let max_value_for_size = shifted.overflowing_sub(1u64).0;
        println!(
            "Range {}..={} ({} bits per int)",
            0, max_value_for_size, num_bits_per_int,
        );

        // Exclusive range
        for _ in 0..num_ints {
            let number: u64 = rng.gen_range(0..=max_value_for_size).try_into().unwrap();
            data.push(number);
            // println!("  ->  {}", number);
        }
        data
    }

    #[test]
    fn decode_1_1() {
        for size in 1..=8 {
            let original_data = &generate_integers(size, 1000);
            let encoded_v1_1_data = &mut Vec::new();
            let decoded_v1_1_data = &mut Vec::new();
            for integer in original_data {
                let _encoded_size = encode_var_uint(encoded_v1_1_data, *integer).unwrap();
                // println!("Encoding integer {}", integer);
            }
            // println!("Encoded bytes: {:x?}", encoded_v1_1_data);
            let mut position: usize = 0;
            let end_of_stream = encoded_v1_1_data.len();
            while position < end_of_stream {
                let (encoded_size, value) =
                    decode_var_uint(&encoded_v1_1_data[position..]).unwrap();
                position += encoded_size;
                decoded_v1_1_data.push(value);
            }
            assert_eq!(decoded_v1_1_data.len(), 1000);
            assert_eq!(decoded_v1_1_data, original_data);
        }
    }

    #[test]
    fn compare_1_0_and_1_1_sizes() {
        let original_data = &generate_integers(8, 10_000);
        let encoded_v1_1_data = &mut Vec::new();
        let encoded_v1_0_data = &mut Vec::new();
        for integer in original_data {
            let encoded_1_1_size = encode_var_uint(encoded_v1_1_data, *integer).unwrap();
            let encoded_1_0_size = VarUInt::write_u64(encoded_v1_0_data, *integer).unwrap();
            assert_eq!(
                encoded_1_0_size,
                encoded_1_1_size,
                "{}: 1.0 size {} != 1.1 size {}\n1.0 bytes: {:#x?}\n1.1 bytes: {:#x?}",
                integer,
                encoded_1_0_size,
                encoded_1_1_size,
                &encoded_v1_0_data[encoded_v1_0_data.len() - encoded_1_0_size..],
                &encoded_v1_1_data[encoded_v1_1_data.len() - encoded_1_1_size..]
            );
            // println!(
            //     "Integer: {}, 1.0 size: {}, 1.1 size: {}",
            //     integer, encoded_1_0_size, encoded_1_1_size
            // );
        }
        // println!("Encoded bytes: {:x?}", encoded_v1_1_data);
        // println!("1.0: {:X?}", encoded_v1_0_data);
        // println!("1.1: {:X?}", encoded_v1_1_data);
        assert_eq!(encoded_v1_0_data.len(), encoded_v1_1_data.len());
    }

    fn test_encode_var_uint(value: u64, expected: &[u8]) {
        let mut buffer: Vec<u8> = Vec::new();
        let encoded_size = encode_var_uint(&mut buffer, value);
        assert!(encoded_size.is_ok());
        assert_eq!(buffer.as_slice(), expected);
    }

    fn test_decode_var_uint(input: &[u8], expected: u64) {
        let (_size_in_bytes, value) = decode_var_uint(input).unwrap();
        assert_eq!(value, expected);
    }

    #[test]
    fn test_decode() {
        // 1-byte values
        test_decode_var_uint(&[0x03], 1);
        test_decode_var_uint(&[0x05], 2);
        test_decode_var_uint(&[0x07], 3);
        test_decode_var_uint(&[0x09], 4);
        test_decode_var_uint(&[0x0B], 5);

        // Maximum value of 7 unsigned bits, which is the largest value that can be encoded in a
        // single byte.
        test_decode_var_uint(&[0xFF], 127u64);
    }

    #[test]
    fn test_encode() {
        // 1-byte values
        test_encode_var_uint(1u64, &[0x03]);
        test_encode_var_uint(2u64, &[0x05]);
        test_encode_var_uint(3u64, &[0x07]);
        test_encode_var_uint(4u64, &[0x09]);
        test_encode_var_uint(5u64, &[0x0B]);

        // Maximum value of 7 unsigned bits, which is the largest value that can be encoded in a
        // single byte.
        test_encode_var_uint(127u64, &[0xFF]);
    }
}
