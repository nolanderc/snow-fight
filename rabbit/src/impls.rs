mod vlq;

use crate::{PackBits, ReadBits, UnpackBits, WriteBits};

impl PackBits for bool {
    fn pack_bits<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        writer.write(*self as u32, 1)
    }
}

impl UnpackBits for bool {
    fn unpack_bits<R>(reader: &mut R) -> Result<Self, R::Error>
    where
        R: ReadBits,
    {
        let value = reader.read(1)? != 0;
        Ok(value)
    }
}

impl PackBits for u8 {
    fn pack_bits<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        writer.write(*self as u32, 8)
    }
}

impl UnpackBits for u8 {
    fn unpack_bits<R>(reader: &mut R) -> Result<Self, R::Error>
    where
        R: ReadBits,
    {
        let value = reader.read(8)? as u8;
        Ok(value)
    }
}

macro_rules! impl_bit_packing_integer {
    ($ty:ty) => {
        impl PackBits for $ty {
            fn pack_bits<W>(&self, writer: &mut W) -> Result<(), W::Error>
            where
                W: WriteBits,
            {
                vlq::VariableLengthQuantity::encode(*self, writer)
            }
        }

        impl UnpackBits for $ty {
            fn unpack_bits<R>(reader: &mut R) -> Result<Self, R::Error>
            where
                R: ReadBits,
            {
                vlq::VariableLengthQuantity::decode(reader)
            }
        }
    };
}

impl_bit_packing_integer!(u16);
impl_bit_packing_integer!(u32);
impl_bit_packing_integer!(u64);
impl_bit_packing_integer!(u128);
impl_bit_packing_integer!(usize);

impl_bit_packing_integer!(i16);
impl_bit_packing_integer!(i32);
impl_bit_packing_integer!(i64);
impl_bit_packing_integer!(i128);
impl_bit_packing_integer!(isize);

impl PackBits for f32 {
    fn pack_bits<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        writer.write(self.to_bits(), 32)
    }
}

impl UnpackBits for f32 {
    fn unpack_bits<R>(reader: &mut R) -> Result<Self, R::Error>
    where
        R: ReadBits,
    {
        reader.read(32).map(f32::from_bits)
    }
}

impl PackBits for f64 {
    fn pack_bits<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        let bits = self.to_bits();
        let high = bits >> 32;
        writer.write(high as u32, 32)?;
        writer.write(bits as u32, 32)?;
        Ok(())
    }
}

impl UnpackBits for f64 {
    fn unpack_bits<R>(reader: &mut R) -> Result<Self, R::Error>
    where
        R: ReadBits,
    {
        let high = reader.read(32)? as u64;
        let low = reader.read(32)? as u64;
        let combined = (high << 32) | low;
        Ok(f64::from_bits(combined))
    }
}

impl<T> PackBits for Option<T>
where
    T: PackBits,
{
    fn pack_bits<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        match self {
            None => writer.write(0, 1),
            Some(value) => {
                writer.write(1, 1)?;
                value.pack_bits(writer)
            }
        }
    }
}

impl<T> UnpackBits for Option<T>
where
    T: UnpackBits,
{
    fn unpack_bits<R>(reader: &mut R) -> Result<Self, R::Error>
    where
        R: ReadBits,
    {
        if reader.read(1)? == 0 {
            Ok(None)
        } else {
            T::unpack_bits(reader).map(Some)
        }
    }
}

impl<T> PackBits for Vec<T>
where
    T: PackBits,
{
    fn pack_bits<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        self.len().pack_bits(writer)?;
        for item in self {
            item.pack_bits(writer)?;
        }
        Ok(())
    }
}

impl<T> UnpackBits for Vec<T>
where
    T: UnpackBits,
{
    fn unpack_bits<R>(reader: &mut R) -> Result<Self, R::Error>
    where
        R: ReadBits,
    {
        let len = usize::unpack_bits(reader)?;
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            let item = T::unpack_bits(reader)?;
            data.push(item);
        }
        Ok(data)
    }
}

impl<T> PackBits for [T]
where
    T: PackBits,
{
    fn pack_bits<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        self.len().pack_bits(writer)?;
        for item in self {
            item.pack_bits(writer)?;
        }
        Ok(())
    }
}
