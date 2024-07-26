use serde::ser::Serialize;
use serde_json::{error::Result, Serializer};

pub fn dedup_to_string_pretty<T>(value: &T) -> Result<String>
where
    T: ?Sized + Serialize,
{
    let mut vec = Vec::with_capacity(128);
    let mut ser = Serializer::pretty(&mut vec);
    value.serialize(&mut ser)?;
    let string = unsafe {
        // We do not emit invalid UTF-8.
        String::from_utf8_unchecked(vec)
    };
    Ok(string)
}
