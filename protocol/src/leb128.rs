use crate::error::{Error, Result};

pub trait EncodeLeb128 {
    fn encode_leb128(self, bytes: &mut Vec<u8>);
}

pub trait DecodeLeb128 {
    fn decode_leb128(bytes: &[u8]) -> Result<(Self, &[u8])>
    where
        Self: Sized;
}

macro_rules! impl_encode_leb128_unsigned {
    ($($ty:ty),*) => { $(
        impl EncodeLeb128 for $ty {
            fn encode_leb128(self, bytes: &mut Vec<u8>) {
                let mut value = self;
                loop {
                    let low = (value & 0x7F) as u8;
                    value >>= 7;

                    if value == 0 {
                        bytes.push(low);
                        break;
                    } else {
                        bytes.push((1 << 7) | low)
                    }
                }
            }
        }
    )*
    }
}

macro_rules! impl_encode_leb128_signed {
    ($($ty:ty),*) => { $(
        impl EncodeLeb128 for $ty {
            fn encode_leb128(self, bytes: &mut Vec<u8>) {
                let mut value = self;
                loop {
                    let low = (value & 0x7F) as u8;
                    value >>= 7;

                    let positive = low & (1 << 6) == 0;
                    let negative = !positive;

                    let only_zeros = value == 0 && positive;
                    let only_ones = value == -1 && negative;

                    if only_zeros || only_ones {
                        bytes.push(low);
                        break;
                    } else {
                        bytes.push((1 << 7) | low)
                    }
                }
            }
        }
        )*
    }
}

macro_rules! impl_decode_leb128_unsigned {
    ($($ty:ty),*) => { $(
        impl DecodeLeb128 for $ty {
            fn decode_leb128(bytes: &[u8]) -> Result<(Self, &[u8])> {
                const BITS: i32 = std::mem::size_of::<$ty>() as i32 * 8;

                let mut value: $ty = 0;
                let mut bytes = bytes.iter();

                let mut shift = 0i32;

                loop {
                    let next = bytes.next().ok_or(Error::Eof)?;

                    value |= (next & 0x7f) as $ty << shift;
                    shift += 7;

                    let has_successor = next & (1 << 7) != 0;

                    // If data could have been shifted outside the integer, make sure that this was
                    // the last byte (since the following bytes would overflow), and that no data
                    // (1's) are lost.
                    let overflow = shift - BITS;
                    if overflow >= 0 {
                        let not_only_zeros = next >> (7 - overflow) != 0;
                        if has_successor || not_only_zeros {
                            return Err(Error::Leb128Overflow);
                        }
                    }

                    if !has_successor {
                        break Ok((value, bytes.as_slice()));
                    }
                }
            }
        }
    )*
    }
}

macro_rules! impl_decode_leb128_signed {
    ($($ty:ty),*) => { $(
        impl DecodeLeb128 for $ty {
            fn decode_leb128(bytes: &[u8]) -> Result<(Self, &[u8])> {
                const BITS: i32 = std::mem::size_of::<$ty>() as i32 * 8;

                let mut value: $ty = 0;
                let mut bytes = bytes.iter();

                let mut shift = 0i32;

                loop {
                    let next = bytes.next().ok_or(Error::Eof)?;

                    let low = next & 0x7f;
                    value |= low as $ty << shift;
                    shift += 7;

                    let has_successor = next & (1 << 7) != 0;

                    let overflow = shift - BITS;
                    if overflow >= 0 {
                        let mask = 0x7f ^ (0x7f >> overflow);
                        let only_zeros = low & mask == 0;
                        let only_ones = low & mask == mask;
                        if has_successor || !(only_zeros || only_ones) {
                            return Err(Error::Leb128Overflow);
                        }
                    }

                    if !has_successor {
                        // sign extend
                        if next & (1 << 6) != 0 && overflow < 0 {
                            dbg!(value, shift);
                            value |= !0 << shift;
                        }

                        break Ok((value, bytes.as_slice()));
                    }
                }
            }
        }
        )*
    }
}

impl_encode_leb128_unsigned!(u8, u16, u32, u64, u128, usize);
impl_encode_leb128_signed!(i8, i16, i32, i64, i128, isize);

impl_decode_leb128_unsigned!(u8, u16, u32, u64, u128, usize);
impl_decode_leb128_signed!(i8, i16, i32, i64, i128, isize);

#[cfg(test)]
mod tests {
    use super::*;

    use std::cmp::PartialEq;
    use std::fmt::Debug;

    fn test_round_trip<T>(value: T)
    where
        T: EncodeLeb128 + DecodeLeb128 + PartialEq + Debug + Copy,
    {
        let mut buffer = Vec::new();
        value.encode_leb128(&mut buffer);

        let (decoded, rest) =
            T::decode_leb128(&buffer).expect(&format!("failed to decode {:?}", value));
        assert_eq!(value, decoded);
        assert!(rest.is_empty());
    }

    #[test]
    fn round_trip_unsigned() {
        test_round_trip(123u32);
        test_round_trip(123456u32);

        test_round_trip(!0u64);

        for i in (!0u64 >> 8) - 16..(!0u64 >> 8) + 16 {
            test_round_trip(i);
        }

        for i in std::u8::MIN..=std::u8::MAX {
            test_round_trip(i);
        }

        for i in std::u16::MIN..=std::u16::MAX {
            test_round_trip(i);
        }
    }

    #[test]
    fn round_trip_signed() {
        test_round_trip(-123i32);
        test_round_trip(-123456i32);

        for i in std::i8::MIN..=std::i8::MAX {
            test_round_trip(i);
        }

        for i in std::i16::MIN..=std::i16::MAX {
            test_round_trip(i);
        }
    }

    #[test]
    fn decode_unsigned_with_overflow() {
        let buffer = [0xff, 0x02];

        assert_eq!(
            u8::decode_leb128(&buffer).unwrap_err(),
            Error::Leb128Overflow
        );
    }

    #[test]
    fn decode_unsigned_with_overflow_continuation() {
        let buffer = [0xff, 0x81, 0x00];

        assert_eq!(
            u8::decode_leb128(&buffer).unwrap_err(),
            Error::Leb128Overflow
        );
    }

    #[test]
    fn decode_signed_with_overflow() {
        let buffer = [0xff, 0x7d];

        assert_eq!(
            i8::decode_leb128(&buffer).unwrap_err(),
            Error::Leb128Overflow
        );
    }

    #[test]
    fn decode_signed_with_overflow_continuation() {
        let buffer = [0xff, 0xff, 0x7f];

        assert_eq!(
            i8::decode_leb128(&buffer).unwrap_err(),
            Error::Leb128Overflow
        );
    }
}
