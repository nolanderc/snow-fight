//! Variable Lenqth Quantity (variable length integer) encoding.
//!
//! See: https://en.wikipedia.org/wiki/Variable-length_quantity
//!
//! Abuses the fact that most integers won't utilize the full width available in the integer's
//! reperesentation (most often 32-bit integers are in the range 0~100).
//!
//!
//! # Representation
//!
//! The number of leading zeros tells us the number of additional bytes (we always need atleast one
//! byte) required to parse the number:
//!
//! ```text
//! 1 additional bytes: 1
//! 2 additional bytes: 01
//! 3 additional bytes: 001
//! 4 additional bytes: 0001
//! 5 additional bytes: 00001
//! 6 additional bytes: 000001
//! 7 additional bytes: 0000001
//! 8 additional bytes: 00000001
//! 9 additional bytes: 000000001
//! etc.
//! ```
//!
//! As a result we can store integers in the range 0-256 (inclusive) using only 9 bits, regardless
//! of the width of the integer type.
//!
//!
//! ## Signed numbers
//!
//! See: https://developers.google.com/protocol-buffers/docs/encoding?csw=1#signed-integers
//!
//! Signed numbers are encoded using a ZigZag encoding. Basically, we encode even numbers as
//! positive and negitive numbers as negative:
//!
//! ```text
//! Encoding    Decoded
//! 0            0
//! 1           -1
//! 2            1
//! 3           -2
//! 4            2
//! 5           -3
//! ...
//! 4294967294   2147483647
//! 4294967295  -2147483648
//! ```

use crate::{read::Error, ReadBits, WriteBits};

pub(crate) trait VariableLengthQuantity: Default + Sized {
    fn encode<W: WriteBits>(self, writer: &mut W) -> Result<(), W::Error>;
    fn decode<R: ReadBits>(reader: &mut R) -> Result<Self, R::Error>;
}

macro_rules! impl_vlq_unsigned {
    ($ty:ty) => {
        impl VariableLengthQuantity for $ty {
            fn encode<W: WriteBits>(self, writer: &mut W) -> Result<(), W::Error> {
                const SIZE: usize = std::mem::size_of::<$ty>();

                let implicit = self.leading_zeros() as usize;
                let additional_bytes = (8 * SIZE - implicit).saturating_sub(1) / 8;

                writer.write(0, additional_bytes as u8)?;
                writer.write(1, 1)?;

                let mut bytes = additional_bytes + 1;

                while bytes > 0 {
                    let stride = usize::min(bytes, 4);
                    bytes -= stride;

                    let bit_offset = 8 * bytes;
                    let window = (self >> bit_offset) as u32;

                    writer.write(window, 8 * stride as u8)?;
                }

                Ok(())
            }

            fn decode<R: ReadBits>(reader: &mut R) -> Result<Self, R::Error> {
                const SIZE: usize = std::mem::size_of::<$ty>();

                // TODO: read multiple bits at once
                let mut bytes = 1;
                loop {
                    let bit = reader.read(1)?;
                    if bit == 0 {
                        bytes += 1;
                        if bytes > SIZE {
                            return Err(R::Error::custom("integer capacity exceeded"));
                        }
                    } else {
                        break;
                    }
                }

                let mut value: $ty = 0;

                while dbg!(bytes) > 0 {
                    let stride = usize::min(bytes, 4);
                    bytes -= stride;

                    let next_bytes = reader.read(8 * stride as u8)?;
                    value = value.checked_shl(8 * stride as u32).unwrap_or(0);
                    value |= next_bytes as $ty;
                }

                Ok(value)
            }
        }
    };
}

impl_vlq_unsigned!(u16);
impl_vlq_unsigned!(u32);
impl_vlq_unsigned!(u64);
impl_vlq_unsigned!(u128);
impl_vlq_unsigned!(usize);

macro_rules! impl_vlq_signed {
    ($signed:ty as $unsigned:ty) => {
        impl VariableLengthQuantity for $signed {
            fn encode<W: WriteBits>(self, writer: &mut W) -> Result<(), W::Error> {
                const BITS: usize = 8 * std::mem::size_of::<$signed>();

                // See: https://en.wikipedia.org/wiki/Variable-length_quantity#Zigzag_encoding
                //
                // We want to store the sign bit in the LSB, so we shift the number up one bit, and
                // due to sign-extension the bits would be flipped by the xor if the number was
                // negative (ones' complement).
                let zig_zag = (self << 1) ^ (self >> (BITS - 1));

                VariableLengthQuantity::encode(zig_zag as $unsigned, writer)
            }

            fn decode<R: ReadBits>(reader: &mut R) -> Result<Self, R::Error> {
                let value = <$unsigned>::decode(reader)?;

                // We store the sign bit in the LSB, so restore all the other bits to their original
                // position. If the sign bit was set, we need to flip all the bits again. Remember
                // that 1 = 0x000001 and -1 = 0xffffff
                let zig_zag = (value >> 1) as $signed ^ -(value as $signed & 1);

                Ok(zig_zag)
            }
        }
    };
}

impl_vlq_signed!(i16 as u16);
impl_vlq_signed!(i32 as u32);
impl_vlq_signed!(i64 as u64);
impl_vlq_signed!(i128 as u128);
impl_vlq_signed!(isize as usize);

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_lossless<T>(value: T)
    where
        T: Copy + VariableLengthQuantity + PartialEq + std::fmt::Debug,
    {
        let mut writer = crate::BitWriter::new();
        value.encode(&mut writer).unwrap();
        let bytes = dbg!(writer.finish());
        let mut reader = crate::BitReader::new(&bytes);
        let encoded = T::decode(&mut reader).unwrap();
        assert_eq!(value, encoded);
    }

    #[test]
    fn encode_lossless_small() {
        for i in 0..512u16 {
            assert_lossless(i);
        }
        for i in 0..512u32 {
            assert_lossless(i);
        }
        for i in 0..512u64 {
            assert_lossless(i);
        }
        for i in 0..512u128 {
            assert_lossless(i);
        }

        for i in -512..512i16 {
            assert_lossless(i);
        }
        for i in -512..512i32 {
            assert_lossless(i);
        }
        for i in -512..512i64 {
            assert_lossless(i);
        }
        for i in -512..512i128 {
            assert_lossless(i);
        }
    }

    #[test]
    fn encode_lossless_large() {
        assert_lossless(u64::max_value());
        assert_lossless(u128::max_value());

        assert_lossless(i64::max_value());
        assert_lossless(i128::max_value());
        assert_lossless(i64::min_value());
        assert_lossless(i128::min_value());
    }
}
