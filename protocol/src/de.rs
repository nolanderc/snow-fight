use serde::de::Deserialize;

pub fn from_bytes<'de, T>(bytes: &[u8]) -> T
where
    T: Deserialize<'de>,
{
    todo!()
}
