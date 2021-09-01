use crate::types::decimal::Decimal;
use crate::types::timestamp::Timestamp;
use std::convert::TryInto;
use std::mem::size_of;

/// Types that do not map nicely to the Serde Data Model
#[derive(Debug, PartialEq)]
pub enum Tunneled<'a> {
    Blob(&'a [u8]),
    Timestamp(&'a Timestamp),
    Decimal(&'a Decimal),
}

const POINTER_SIZE: usize = size_of::<usize>();

/// This is a hack. TODO: Explain.
/// Some Ion data types[1] do not have meaningful counterparts in the Serde
/// data model[2].
/// Converting a pointer to a byte array allows us to tunnel any given value into
/// an Ion-centric implementation of Serde's Serializer trait.
/// [1] https://amzn.github.io/ion-docs/guides/why.html#rich-type-system
/// [2] https://serde.rs/data-model.html
impl<'a> Tunneled<'a> {
    /// Returns a byte array representation of the address pointed to by &self
    pub fn serialize_ref(&self) -> [u8; POINTER_SIZE] {
        // Get a pointer to `self`. This is safe until we dereference it.
        let self_ptr: *const Self = self;
        unsafe {
            // Transmute the pointer into a byte array.
            std::mem::transmute(self_ptr)
        }
    }

    /// Turns a slice containing a serialized pointer back into
    /// a borrowed reference to Self.
    pub fn deserialize_ref(bytes: &[u8]) -> &Self {
        assert_eq!(POINTER_SIZE, bytes.len());
        let array: [u8; POINTER_SIZE] = bytes.try_into().expect("Conversion failed");
        let usize_ptr = usize::from_ne_bytes(array);
        let tunneled_ptr = usize_ptr as *const Self;
        unsafe {
            // let tunneled: Tunneled = std::ptr::read(tunneled_ptr);
            let tunneled: &Tunneled = &*tunneled_ptr;
            tunneled
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_tunneled_value(original_ref: &Tunneled) {
        let ptr_bytes = original_ref.serialize_ref();
        let roundtripped_ref = Tunneled::deserialize_ref(&ptr_bytes[..]);
        assert_eq!(original_ref, roundtripped_ref);
    }

    #[test]
    fn roundtrip_timestamp() {
        let timestamp = Timestamp::with_ymd(2021, 9, 7)
            .with_hms(21, 23, 30)
            .with_milliseconds(0)
            .build_at_unknown_offset()
            .unwrap();
        let tunneled_ts = Tunneled::Timestamp(&timestamp);
        roundtrip_tunneled_value(&tunneled_ts);
    }

    #[test]
    fn roundtrip_decimal() {
        let decimal: Decimal = 105.into();
        let tunneled_decimal = Tunneled::Decimal(&decimal);
        roundtrip_tunneled_value(&tunneled_decimal);
    }

    #[test]
    fn roundtrip_blob() {
        let data = vec![1, 2, 3, 4, 5];
        let tunneled_blob = Tunneled::Blob(&data);
        roundtrip_tunneled_value(&tunneled_blob);
    }
}
