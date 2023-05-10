use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ion_rs::binary::var_uint::VarUInt;
use ion_rs::ion_1_1::{alt, decode_var_uint, encode_var_uint};
use rand::prelude::*;

// Like the `ion_1_1-primitives` benchmark, but uses zero bits for "not end" and 1 for "end"
// instead of 1 for "continue" and zero for "not continue". This is expected to remove a small
// amount of overhead because x86 has a "count trailing zeros" intrinsic.

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

fn roundtrip_alt_var_uint(c: &mut Criterion) {
    const NUM_INTEGERS: usize = 10_000;
    for int_size_in_bytes in 1..=8 {
        let original_data = generate_integers(int_size_in_bytes, NUM_INTEGERS);
        let encoded_length = original_data.len() * int_size_in_bytes;

        let encoded_v1_0_data: &mut Vec<u8> = &mut Vec::with_capacity(encoded_length);
        let decoded_v1_0_data: &mut Vec<u64> = &mut Vec::with_capacity(NUM_INTEGERS);
        let encoded_v1_1_data: &mut Vec<u8> = &mut Vec::with_capacity(encoded_length);
        let decoded_v1_1_data: &mut Vec<u64> = &mut Vec::with_capacity(NUM_INTEGERS);
        let encoded_v1_1_alt_data: &mut Vec<u8> = &mut Vec::with_capacity(encoded_length);
        let decoded_v1_1_alt_data: &mut Vec<u64> = &mut Vec::with_capacity(NUM_INTEGERS);

        let mut group = c.benchmark_group("VarUInt Encoding");

        group.bench_with_input(
            BenchmarkId::new("Ion v1.0", int_size_in_bytes),
            original_data.as_slice(),
            |b, original_data| {
                b.iter(|| {
                    encoded_v1_0_data.clear();
                    for integer in original_data {
                        let _encoded_size =
                            VarUInt::write_u64(encoded_v1_0_data, *integer).unwrap();
                    }
                    black_box(encoded_v1_0_data.len());
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("Ion v1.1", int_size_in_bytes),
            original_data.as_slice(),
            |b, original_data| {
                b.iter(|| {
                    encoded_v1_1_data.clear();
                    for integer in original_data {
                        let _encoded_size = encode_var_uint(encoded_v1_1_data, *integer).unwrap();
                    }
                    black_box(encoded_v1_1_data.len());
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("Ion v1.1 Alt", int_size_in_bytes),
            original_data.as_slice(),
            |b, original_data| {
                b.iter(|| {
                    encoded_v1_1_alt_data.clear();
                    for integer in original_data {
                        let _encoded_size =
                            alt::encode_var_uint(encoded_v1_1_alt_data, *integer).unwrap();
                    }
                    black_box(encoded_v1_1_alt_data.len());
                });
            },
        );

        group.finish();

        let mut group = c.benchmark_group("VarUInt Decoding");

        group.bench_with_input(
            BenchmarkId::new("Ion v1.0", int_size_in_bytes),
            encoded_v1_0_data.as_slice(),
            |b, encoded_v1_0_data| {
                b.iter(|| {
                    decoded_v1_0_data.clear();
                    let input = &mut std::io::Cursor::new(encoded_v1_0_data);
                    let end_of_stream = encoded_v1_0_data.len() as u64;
                    while input.position() < end_of_stream {
                        let decoded_var_uint = VarUInt::read(input).unwrap();
                        decoded_v1_0_data.push(decoded_var_uint.value() as u64);
                    }
                    black_box(decoded_v1_0_data.len());
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("Ion v1.1", int_size_in_bytes),
            encoded_v1_1_data.as_slice(),
            |b, encoded_v1_1_data| {
                b.iter(|| {
                    decoded_v1_1_data.clear();
                    let mut position: usize = 0;
                    let end_of_stream = encoded_v1_1_data.len();
                    while position < end_of_stream {
                        let (encoded_size, value) =
                            decode_var_uint(&encoded_v1_1_data[position..]).unwrap();
                        position += encoded_size;
                        decoded_v1_1_data.push(value);
                    }
                    black_box(decoded_v1_1_data.len());
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("Ion v1.1 Alt", int_size_in_bytes),
            encoded_v1_1_alt_data.as_slice(),
            |b, encoded_v1_1_alt_data| {
                b.iter(|| {
                    decoded_v1_1_alt_data.clear();
                    let mut position: usize = 0;
                    let end_of_stream = encoded_v1_1_alt_data.len();
                    while position < end_of_stream {
                        let (encoded_size, value) =
                            alt::decode_var_uint(&encoded_v1_1_alt_data[position..]).unwrap();
                        position += encoded_size;
                        decoded_v1_1_alt_data.push(value);
                    }
                    black_box(decoded_v1_1_alt_data.len());
                });
            },
        );

        group.finish();

        println!("1.0 values: {}", decoded_v1_0_data.len());
        println!("1.1 values: {}", decoded_v1_1_data.len());
        println!("1.1 alt vs: {}", decoded_v1_1_alt_data.len());

        // There is no difference in the two formats' encoded sizes
        assert_eq!(encoded_v1_0_data.len(), encoded_v1_1_data.len());
        // We decoded the same number of integers
        assert_eq!(decoded_v1_0_data.len(), decoded_v1_1_data.len());
        // We decoded the same number of integers
        assert_eq!(decoded_v1_1_data.len(), decoded_v1_1_alt_data.len());
        // The decoded integers are the same as the original integers
        assert_eq!(decoded_v1_0_data, &original_data);
        assert_eq!(decoded_v1_1_data, &original_data);
        assert_eq!(decoded_v1_1_alt_data, &original_data);
    }
}

criterion_group!(benches, roundtrip_alt_var_uint);
criterion_main!(benches);
