mod vlq;

use crate::{read::Error as _, PackBits, ReadBits, UnpackBits, WriteBits};

use std::rc::Rc;
use std::sync::Arc;

impl PackBits for bool {
    fn pack<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        writer.write(*self as u32, 1)
    }
}

impl UnpackBits for bool {
    fn unpack<R>(reader: &mut R) -> Result<Self, R::Error>
    where
        R: ReadBits,
    {
        let value = reader.read(1)? != 0;
        Ok(value)
    }
}

impl PackBits for u8 {
    fn pack<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        writer.write(*self as u32, 8)
    }
}

impl UnpackBits for u8 {
    fn unpack<R>(reader: &mut R) -> Result<Self, R::Error>
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
            fn pack<W>(&self, writer: &mut W) -> Result<(), W::Error>
            where
                W: WriteBits,
            {
                vlq::VariableLengthQuantity::encode(*self, writer)
            }
        }

        impl UnpackBits for $ty {
            fn unpack<R>(reader: &mut R) -> Result<Self, R::Error>
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
    fn pack<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        writer.write(self.to_bits(), 32)
    }
}

impl UnpackBits for f32 {
    fn unpack<R>(reader: &mut R) -> Result<Self, R::Error>
    where
        R: ReadBits,
    {
        reader.read(32).map(f32::from_bits)
    }
}

impl PackBits for f64 {
    fn pack<W>(&self, writer: &mut W) -> Result<(), W::Error>
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
    fn unpack<R>(reader: &mut R) -> Result<Self, R::Error>
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
    fn pack<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        match self {
            None => writer.write(0, 1),
            Some(value) => {
                writer.write(1, 1)?;
                value.pack(writer)
            }
        }
    }
}

impl<T> UnpackBits for Option<T>
where
    T: UnpackBits,
{
    fn unpack<R>(reader: &mut R) -> Result<Self, R::Error>
    where
        R: ReadBits,
    {
        if reader.read(1)? == 0 {
            Ok(None)
        } else {
            T::unpack(reader).map(Some)
        }
    }
}

impl<T> PackBits for Vec<T>
where
    T: PackBits,
{
    fn pack<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        (self.len() as u32).pack(writer)?;
        for item in self {
            item.pack(writer)?;
        }
        Ok(())
    }
}

impl<T> UnpackBits for Vec<T>
where
    T: UnpackBits,
{
    fn unpack<R>(reader: &mut R) -> Result<Self, R::Error>
    where
        R: ReadBits,
    {
        let len = u32::unpack(reader)?;
        let mut data = Vec::with_capacity(len as usize);
        for _ in 0..len {
            let item = T::unpack(reader)?;
            data.push(item);
        }
        Ok(data)
    }
}

impl<T> PackBits for [T]
where
    T: PackBits,
{
    fn pack<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        (self.len() as u32).pack(writer)?;
        for item in self {
            item.pack(writer)?;
        }
        Ok(())
    }
}

// TODO: based on the length of the string, sacrifice compactness for byte alignment
impl PackBits for String {
    fn pack<W>(&self, writer: &mut W) -> Result<(), W::Error>
    where
        W: WriteBits,
    {
        self.as_bytes().pack(writer)
    }
}

// TODO: based on the length of the string, sacrifice compactness for byte alignment
impl UnpackBits for String {
    fn unpack<R>(reader: &mut R) -> Result<Self, R::Error>
    where
        R: ReadBits,
    {
        let bytes = Vec::<u8>::unpack(reader)?;
        String::from_utf8(bytes).map_err(R::Error::custom)
    }
}

macro_rules! impl_wrapper {
    ($wrapper:ident) => {
        impl<T> PackBits for $wrapper<T>
        where
            T: PackBits,
        {
            fn pack<W>(&self, writer: &mut W) -> Result<(), W::Error>
            where
                W: WriteBits,
            {
                self.as_ref().pack(writer)
            }
        }

        impl<T> UnpackBits for $wrapper<T>
        where
            T: UnpackBits,
        {
            fn unpack<R>(reader: &mut R) -> Result<Self, R::Error>
            where
                R: ReadBits,
            {
                T::unpack(reader).map($wrapper::new)
            }
        }
    };
}

impl_wrapper!(Box);
impl_wrapper!(Arc);
impl_wrapper!(Rc);

macro_rules! impl_bit_packing_tuple {
    ($($ident:ident),+) => {
        impl<$($ident: PackBits),*> PackBits for ($($ident,)*) {
            #[allow(non_snake_case)]
            fn pack<W: WriteBits>(&self, writer: &mut W) -> Result<(), W::Error> {
                let ($($ident,)*) = self;
                $( $ident.pack(writer)?;)*
                Ok(())
            }
        }

        impl<$($ident: UnpackBits),*> UnpackBits for ($($ident,)*) {
            fn unpack<R: ReadBits>(reader: &mut R) -> Result<Self, R::Error> {
                Ok(($( $ident::unpack(reader)? ,)*))
            }
        }
    };
}

impl_bit_packing_tuple!(A);
impl_bit_packing_tuple!(A, B);
impl_bit_packing_tuple!(A, B, C);
impl_bit_packing_tuple!(A, B, C, D);
impl_bit_packing_tuple!(A, B, C, D, E);
