use crate::error::{Error, Result};
use crate::leb128::DecodeLeb128;
use serde::de::{self, Deserialize, IntoDeserializer, Visitor};
use std::convert::TryInto;

pub fn from_bytes<'de, T>(bytes: &'de [u8]) -> Result<T>
where
    T: Deserialize<'de>,
{
    let mut deserializer = Deserializer::from_bytes(bytes);
    let value = T::deserialize(&mut deserializer)?;

    if deserializer.is_empty() {
        Ok(value)
    } else {
        Err(Error::TrailingBytes)
    }
}

pub struct Deserializer<'de> {
    bytes: &'de [u8],
}

impl<'de> Deserializer<'de> {
    pub fn from_bytes(bytes: &'de [u8]) -> Self {
        Deserializer { bytes }
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Get the next byte in the byte stream without advancing.
    pub fn peek_byte(&self) -> Result<u8> {
        self.bytes.first().copied().ok_or(Error::Eof)
    }

    /// Get the next byte in the byte stream and advance.
    pub fn next_byte(&mut self) -> Result<u8> {
        let (next, rest) = self.bytes.split_first().ok_or(Error::Eof)?;
        self.bytes = rest;
        Ok(*next)
    }

    /// Get the next n bytes in the byte stream and advance.
    pub fn next_bytes(&mut self, n: usize) -> Result<&[u8]> {
        if n > self.bytes.len() {
            Err(Error::Eof)
        } else {
            let (next, rest) = self.bytes.split_at(n);
            self.bytes = rest;
            Ok(next)
        }
    }

    pub fn parse_leb128<T>(&mut self) -> Result<T>
    where
        T: DecodeLeb128,
    {
        let (value, rest) = T::decode_leb128(&self.bytes)?;
        self.bytes = rest;
        Ok(value)
    }

    pub fn parse_bytes(&mut self) -> Result<&[u8]> {
        let len = self.parse_leb128()?;
        self.next_bytes(len)
    }

    pub fn parse_str(&mut self) -> Result<&str> {
        let bytes = self.parse_bytes()?;
        std::str::from_utf8(bytes).map_err(Into::into)
    }

    pub fn parse_bool(&mut self) -> Result<bool> {
        match self.next_byte()? {
            0 => Ok(false),
            1 => Ok(true),
            byte => Err(Error::InvalidBool(byte)),
        }
    }
}

impl<'a, 'de> de::Deserializer<'de> for &'a mut Deserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let _ = visitor;
        Err(Error::UnknownType)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.parse_bool()?;
        visitor.visit_bool(value)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.next_byte()?;
        visitor.visit_i8(value as i8)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.parse_leb128()?;
        visitor.visit_i16(value)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.parse_leb128()?;
        visitor.visit_i32(value)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.parse_leb128()?;
        visitor.visit_i64(value)
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.parse_leb128()?;
        visitor.visit_i128(value)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.next_byte()?;
        visitor.visit_u8(value)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.parse_leb128()?;
        visitor.visit_u16(value)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.parse_leb128()?;
        visitor.visit_u32(value)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.parse_leb128()?;
        visitor.visit_u64(value)
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.parse_leb128()?;
        visitor.visit_u128(value)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let bytes = self.next_bytes(4)?;
        let buffer = bytes.try_into().unwrap();
        visitor.visit_f32(f32::from_be_bytes(buffer))
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let bytes = self.next_bytes(8)?;
        let buffer = bytes.try_into().unwrap();
        visitor.visit_f32(f32::from_be_bytes(buffer))
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let text = self.parse_str()?;
        let mut chars = text.chars();
        let first = chars.next().ok_or(Error::EmptyString)?;
        if chars.next().is_some() {
            Err(Error::MultiCharString)
        } else {
            visitor.visit_char(first)
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let text = self.parse_str()?;
        visitor.visit_str(text)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let text = self.parse_str()?;
        visitor.visit_string(text.into())
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let bytes = self.parse_bytes()?;
        visitor.visit_bytes(bytes)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let bytes = self.parse_bytes()?;
        visitor.visit_byte_buf(bytes.to_vec())
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let is_some = self.parse_bool()?;
        if is_some {
            visitor.visit_some(self)
        } else {
            visitor.visit_none()
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let _ = name;
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let _ = name;
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let len = self.parse_leb128()?;
        visitor.visit_seq(Sequence::new(self, len))
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(Sequence::new(self, len))
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let _ = name;
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let len = self.parse_leb128()?;
        visitor.visit_map(Sequence::new(self, len))
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple_struct(name, fields.len(), visitor)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let _ = (name, variants);
        visitor.visit_enum(Enum::new(self))
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let _ = visitor;
        Err(Error::IdentifierNotSupported)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let _ = visitor;
        Err(Error::IgnoredNotSupported)
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

struct Sequence<'a, 'de> {
    len: usize,
    deserializer: &'a mut Deserializer<'de>,
}

impl<'a, 'de> Sequence<'a, 'de> {
    pub fn new(deserializer: &'a mut Deserializer<'de>, len: usize) -> Self {
        Sequence { len, deserializer }
    }
}

impl<'a, 'de> de::SeqAccess<'de> for Sequence<'a, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: de::DeserializeSeed<'de>,
    {
        if self.len == 0 {
            Ok(None)
        } else {
            self.len -= 1;
            let value = seed.deserialize(&mut *self.deserializer)?;
            Ok(Some(value))
        }
    }
}

impl<'a, 'de> de::MapAccess<'de> for Sequence<'a, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: de::DeserializeSeed<'de>,
    {
        if self.len == 0 {
            Ok(None)
        } else {
            self.len -= 1;
            let value = seed.deserialize(&mut *self.deserializer)?;
            Ok(Some(value))
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: de::DeserializeSeed<'de>,
    {
        seed.deserialize(&mut *self.deserializer)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.len)
    }
}

struct Enum<'a, 'de> {
    deserializer: &'a mut Deserializer<'de>,
}

impl<'a, 'de> Enum<'a, 'de> {
    pub fn new(deserializer: &'a mut Deserializer<'de>) -> Self {
        Enum { deserializer }
    }
}

impl<'a, 'de> de::EnumAccess<'de> for Enum<'a, 'de> {
    type Error = Error;
    type Variant = &'a mut Deserializer<'de>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant)>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant_index = self.deserializer.parse_leb128::<u32>()?;
        seed.deserialize(variant_index.into_deserializer())
            .map(|value| (value, self.deserializer))
    }
}

impl<'a, 'de> de::VariantAccess<'de> for &'a mut Deserializer<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: de::DeserializeSeed<'de>,
    {
        seed.deserialize(self)
    }

    fn tuple_variant<V>(self, len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_tuple(self, len, visitor)
    }

    fn struct_variant<V>(self, fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.tuple_variant(fields.len(), visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    fn assert_round_trip<T>(value: &T)
    where
        T: Serialize + for<'a> Deserialize<'a> + PartialEq + std::fmt::Debug,
    {
        let bytes = crate::ser::to_bytes(value).unwrap();
        let result: T = from_bytes(&bytes).unwrap();
        assert_eq!(value, &result);
    }

    #[test]
    fn round_trip_struct() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Person {
            age: u32,
            name: String,
        }

        let person = Person {
            age: 42,
            name: "John".into(),
        };

        assert_round_trip(&person)
    }

    #[test]
    fn round_trip_enum() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        enum Molecule {
            Dna(String),
            Protein { name: String, sequence: String },
        }

        let dna = Molecule::Dna("ACGT".into());
        assert_round_trip(&dna);

        let protein = Molecule::Protein {
            name: "<insert name here>".into(),
            sequence: "ARN".into(),
        };
        assert_round_trip(&protein);
    }

    #[test]
    fn round_trip_sequence() {
        let list = [0, 123, 18231237, -472185];
        assert_round_trip(&list);
    }

    #[test]
    fn round_trip_optional() {
        let some = Some("hello".to_owned());
        assert_round_trip(&some);

        let none = Option::<String>::None;
        assert_round_trip(&none);
    }

    #[test]
    fn round_trip_nested() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Bar {
            value: u32,
        }

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        enum Baz {
            A,
            B,
            C,
        }

        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Foo {
            bar: Bar,
            baz: Baz,
        }

        let foo = Foo {
            bar: Bar { value: 14 },
            baz: Baz::B,
        };

        assert_round_trip(&foo);
    }
}
