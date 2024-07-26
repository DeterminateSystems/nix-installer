use serde::ser::{self, Serialize};
use serde_json::{
    error::{Error, Result},
    ser::{CompactFormatter, Compound, Formatter, Serializer},
};
use std::io;

pub fn dedup_to_string_pretty<T>(value: &T) -> Result<String>
where
    T: ?Sized + Serialize,
{
    let mut vec = Vec::with_capacity(128);
    let ser_base = Serializer::pretty(&mut vec);
    let mut ser = DedupSerializer { ser: ser_base };
    println!("created DedupSerializer");
    value.serialize(&mut ser)?;
    let string = unsafe {
        // We do not emit invalid UTF-8.
        String::from_utf8_unchecked(vec)
    };
    Ok(string)
}

struct DedupSerializeMap<'a, W, F>
where
    W: io::Write,
    F: Formatter,
{
    delegate: Compound<'a, W, F>,
    was_action_name: bool,
}

impl<'a, W, F> DedupSerializeMap<'a, W, F>
where
    W: io::Write,
    F: Formatter,
{
    fn new(delegate: Compound<'a, W, F>) -> DedupSerializeMap<'a, W, F> {
        DedupSerializeMap {
            delegate,
            was_action_name: false,
        }
    }
}

impl<'a, W, F> ser::SerializeMap for DedupSerializeMap<'a, W, F>
where
    W: io::Write,
    F: Formatter,
{
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> std::result::Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        println!("serialize_key called");
        self.delegate.serialize_key(key)
    }

    fn serialize_value<T>(&mut self, value: &T) -> std::result::Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        self.delegate.serialize_value(value)
    }

    fn end(self) -> std::result::Result<Self::Ok, Self::Error> {
        self.delegate.end()
    }
}

impl<'a, W, F> ser::SerializeStruct for DedupSerializeMap<'a, W, F>
where
    W: io::Write,
    F: Formatter,
{
    type Ok = ();
    type Error = Error;

    #[inline]
    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        println!("serialize_field({}, value) called", key);
        ser::SerializeMap::serialize_entry(self, key, value)
    }

    #[inline]
    fn end(self) -> Result<()> {
        ser::SerializeMap::end(self)
    }
}

struct DedupSerializer<W, F = CompactFormatter> {
    ser: Serializer<W, F>,
}

impl<'a, W, F> ser::Serializer for &'a mut DedupSerializer<W, F>
where
    W: io::Write,
    F: Formatter,
{
    type Ok = ();
    type Error = Error;

    type SerializeSeq = Compound<'a, W, F>;
    type SerializeTuple = Compound<'a, W, F>;
    type SerializeTupleStruct = Compound<'a, W, F>;
    type SerializeTupleVariant = Compound<'a, W, F>;
    type SerializeMap = DedupSerializeMap<'a, W, F>;
    type SerializeStruct = DedupSerializeMap<'a, W, F>;
    type SerializeStructVariant = Compound<'a, W, F>;

    #[inline]
    fn serialize_bool(self, value: bool) -> Result<()> {
        self.ser.serialize_bool(value)
    }

    #[inline]
    fn serialize_i8(self, value: i8) -> Result<()> {
        self.ser.serialize_i8(value)
    }

    #[inline]
    fn serialize_i16(self, value: i16) -> Result<()> {
        self.ser.serialize_i16(value)
    }

    #[inline]
    fn serialize_i32(self, value: i32) -> Result<()> {
        self.ser.serialize_i32(value)
    }

    #[inline]
    fn serialize_i64(self, value: i64) -> Result<()> {
        self.ser.serialize_i64(value)
    }

    #[inline]
    fn serialize_i128(self, value: i128) -> Result<()> {
        self.ser.serialize_i128(value)
    }

    #[inline]
    fn serialize_u8(self, value: u8) -> Result<()> {
        self.ser.serialize_u8(value)
    }

    #[inline]
    fn serialize_u16(self, value: u16) -> Result<()> {
        self.ser.serialize_u16(value)
    }

    #[inline]
    fn serialize_u32(self, value: u32) -> Result<()> {
        self.ser.serialize_u32(value)
    }

    #[inline]
    fn serialize_u64(self, value: u64) -> Result<()> {
        self.ser.serialize_u64(value)
    }

    #[inline]
    fn serialize_u128(self, value: u128) -> Result<()> {
        self.ser.serialize_u128(value)
    }

    #[inline]
    fn serialize_f32(self, value: f32) -> Result<()> {
        self.ser.serialize_f32(value)
    }

    #[inline]
    fn serialize_f64(self, value: f64) -> Result<()> {
        self.ser.serialize_f64(value)
    }

    #[inline]
    fn serialize_char(self, value: char) -> Result<()> {
        self.ser.serialize_char(value)
    }

    #[inline]
    fn serialize_str(self, value: &str) -> Result<()> {
        self.ser.serialize_str(value)
    }

    #[inline]
    fn serialize_bytes(self, value: &[u8]) -> Result<()> {
        self.ser.serialize_bytes(value)
    }

    #[inline]
    fn serialize_unit(self) -> Result<()> {
        self.ser.serialize_unit()
    }

    #[inline]
    fn serialize_unit_struct(self, name: &'static str) -> Result<()> {
        self.ser.serialize_unit_struct(name)
    }

    #[inline]
    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.ser
            .serialize_unit_variant(name, variant_index, variant)
    }

    #[inline]
    fn serialize_newtype_struct<T>(self, name: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.ser.serialize_newtype_struct(name, value)
    }

    #[inline]
    fn serialize_newtype_variant<T>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.ser
            .serialize_newtype_variant(name, variant_index, variant, value)
    }

    #[inline]
    fn serialize_none(self) -> Result<()> {
        self.ser.serialize_none()
    }

    #[inline]
    fn serialize_some<T>(self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.ser.serialize_some(value)
    }

    #[inline]
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        self.ser.serialize_seq(len)
    }

    #[inline]
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.ser.serialize_tuple(len)
    }

    #[inline]
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.ser.serialize_tuple_struct(name, len)
    }

    #[inline]
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.ser
            .serialize_tuple_variant(name, variant_index, variant, len)
    }

    #[inline]
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap> {
        Ok(DedupSerializeMap::new(self.ser.serialize_map(len)?))
    }

    #[inline]
    fn serialize_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        Ok(DedupSerializeMap::new(
            self.ser.serialize_struct(name, len)?,
        ))
    }

    #[inline]
    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.ser
            .serialize_struct_variant(name, variant_index, variant, len)
    }
}
