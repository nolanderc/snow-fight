use rabbit::{PackBits, ReadBits, UnpackBits, WriteBits};

pub mod point {
    use super::*;
    use cgmath::Point3;

    pub fn pack<W: WriteBits, T: PackBits>(point: &Point3<T>, writer: &mut W) -> Result<(), W::Error> {
        point.x.pack(writer)?;
        point.y.pack(writer)?;
        point.z.pack(writer)?;
        Ok(())
    }

    pub fn unpack<R: ReadBits, T: UnpackBits>(reader: &mut R) -> Result<Point3<T>, R::Error> {
        let x = T::unpack(reader)?;
        let y = T::unpack(reader)?;
        let z = T::unpack(reader)?;
        Ok(Point3 { x, y, z })
    }
}
