use crate::IonResult;

/**
 * This trait captures the format-agnostic parser functionality needed to navigate within an Ion
 * stream and read the values encountered into native Rust data types.
 *
 * Once a value has successfully been read from the stream using one of the read_* functions,
 * calling that function again may return an Err. This is left to the discretion of the implementor.
 */
pub trait IonReader {
    /// The type returned by calls to [Self::next], indicating the next entity in the stream.
    /// Reader implementations representing different levels of abstraction will surface
    /// different sets of encoding artifacts. While an application-level Reader would only surface
    /// Ion values, a lower level Reader might surface symbol tables, Ion version markers, etc.
    type Item<'a>
    where
        Self: 'a;

    /// Returns the (major, minor) version of the Ion stream being read. If ion_version is called
    /// before an Ion Version Marker has been read, the version (1, 0) will be returned.
    fn ion_version(&self) -> (u8, u8);

    /// Attempts to advance the cursor to the next value in the stream at the current depth.
    /// If no value is encountered, returns None; otherwise, returns the Ion type of the next value.
    fn next(&mut self) -> IonResult<Self::Item<'_>>;
}
