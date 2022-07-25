//! [serde](https://crates.io/crates/serde) utilities for lua

use super::*;
use crate::{ffi::*, CRegVal, FromLua, State, ToLua, Type, ValRef};
use alloc::fmt::{self, Display};
#[rustfmt::skip]
use ::serde::{
    de::{Deserialize, DeserializeSeed, Deserializer, Error as DeErr, MapAccess, SeqAccess, Visitor},
    ser::{
        Error, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
        SerializeTuple, SerializeTupleStruct, SerializeTupleVariant, Serializer,
    },
};

#[derive(Clone, Debug, PartialEq, Display)]
pub enum DesErr {
    // One or more variants that can be created by data structures through the
    // `ser::Error` and `de::Error` traits. For example the Serialize impl for
    // Mutex<T> might return an error because the mutex is poisoned, or the
    // Deserialize impl for a struct may return an error because a required
    // field is missing.
    Message(String),

    // Zero or more variants that can be created directly by the Serializer and
    // Deserializer without going through `ser::Error` and `de::Error`. These
    // are specific to the format, in this case JSON.
    Eof,
    Syntax,
    ExpectedBoolean,
    ExpectedInteger,
    ExpectedString,
    ExpectedNull,
    ExpectedArray,
    ExpectedArrayComma,
    ExpectedArrayEnd,
    ExpectedMap,
    ExpectedMapColon,
    ExpectedMapComma,
    ExpectedMapEnd,
    ExpectedEnum,
    TrailingCharacters,
}

#[cfg(feature = "std")]
impl std::error::Error for DesErr {}

impl DeErr for DesErr {
    fn custom<T: Display>(msg: T) -> Self {
        DesErr::Message(msg.to_string())
    }
}

impl State {
    /// convert a serializable value into a lua value
    #[inline(always)]
    pub fn push_serialize<V: Serialize>(&self, v: V) -> Result<(), core::fmt::Error> {
        v.serialize(LuaSerializer(self))
    }

    /// transcode a serializable value from deserializer into a lua value
    #[inline(always)]
    pub fn push_from_deserializer<'de, D: Deserializer<'de>>(
        &self,
        deserializer: D,
    ) -> Result<(), core::fmt::Error> {
        serde_transcode::transcode(deserializer, LuaSerializer(self))
    }
}

/// Represents a value from deserializer will be converted to lua
pub struct FromDeserializer<D>(pub D);

impl<'de, D: Deserializer<'de>> ToLua for FromDeserializer<D> {
    type Error = core::fmt::Error;

    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.check_result(s.push_from_deserializer(self.0));
    }

    #[inline(always)]
    fn to_lua_result(self, s: &State) -> Result<(), Self::Error> {
        s.push_from_deserializer(self.0)?;
        Ok(())
    }
}

/// Wrapper to serializable value
#[derive(Copy, Clone, Deref, DerefMut)]
pub struct SerdeValue<T>(pub T);

impl<T: Serialize> ToLua for SerdeValue<T> {
    type Error = core::fmt::Error;

    #[inline(always)]
    fn to_lua(self, s: &State) {
        s.check_result(s.push_serialize(self.0));
    }

    #[inline(always)]
    fn to_lua_result(self, s: &State) -> Result<(), Self::Error> {
        s.push_serialize(self.0)?;
        Ok(())
    }
}

impl<'a, T: Deserialize<'a> + 'a> FromLua<'a> for SerdeValue<T> {
    #[inline(always)]
    fn from_lua(s: &'a State, i: i32) -> Option<SerdeValue<T>> {
        Some(SerdeValue(T::deserialize(s.val(i)).ok()?))
    }
}

impl<'a> ValRef<'a> {
    /// deserialize a lua value
    #[inline(always)]
    pub fn deserialize<T: Deserialize<'a>>(&self) -> Result<T, DesErr> {
        T::deserialize(*self)
    }

    /// transcode a lua value to another serialize format
    #[inline(always)]
    pub fn transcode<S: Serializer>(self, serializer: S) -> Result<S::Ok, S::Error> {
        serde_transcode::transcode(self, serializer)
    }
}

struct LuaSerializer<'a>(&'a State);
struct LuaTableSerializer<'a>(&'a State, i64);

impl SerializeSeq for LuaTableSerializer<'_> {
    type Ok = ();
    type Error = core::fmt::Error;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.1 += 1;
        self.0.push_serialize(value)?;
        self.0.raw_seti(-2, self.1);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeTuple for LuaTableSerializer<'_> {
    type Ok = ();
    type Error = core::fmt::Error;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeSeq::end(self)
    }
}

impl SerializeTupleStruct for LuaTableSerializer<'_> {
    type Ok = ();
    type Error = core::fmt::Error;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeSeq::end(self)
    }
}

impl SerializeTupleVariant for LuaTableSerializer<'_> {
    type Ok = ();
    type Error = core::fmt::Error;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        SerializeTupleStruct::serialize_field(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeTupleStruct::end(self)
    }
}

impl<'a> LuaTableSerializer<'a> {
    fn begin(s: &'a State, len: usize) -> Self {
        s.create_table(0, len as _);
        Self(s, 0)
    }

    fn begin_array(s: &'a State, len: usize) -> Self {
        s.create_table(len as _, 0);
        Self(s, 0)
    }
}

impl SerializeStruct for LuaTableSerializer<'_> {
    type Ok = ();
    type Error = core::fmt::Error;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.0.push(key);
        SerializeMap::serialize_value(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeMap::end(self)
    }
}

impl SerializeMap for LuaTableSerializer<'_> {
    type Ok = ();
    type Error = core::fmt::Error;

    fn serialize_key<T: ?Sized>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.0.push_serialize(key)
    }

    fn serialize_value<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.0.push_serialize(value)?;
        self.0.raw_set(-3);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeStructVariant for LuaTableSerializer<'_> {
    type Ok = ();
    type Error = core::fmt::Error;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        SerializeStruct::serialize_field(self, key, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeStruct::end(self)
    }
}

impl<'a> Serializer for LuaSerializer<'a> {
    type Ok = ();
    type Error = core::fmt::Error;
    type SerializeSeq = LuaTableSerializer<'a>;
    type SerializeMap = LuaTableSerializer<'a>;
    type SerializeTuple = LuaTableSerializer<'a>;
    type SerializeStruct = LuaTableSerializer<'a>;
    type SerializeStructVariant = LuaTableSerializer<'a>;
    type SerializeTupleStruct = LuaTableSerializer<'a>;
    type SerializeTupleVariant = LuaTableSerializer<'a>;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        let mut dst = [0u8; 100];
        self.0.push(v.encode_utf8(&mut dst).as_bytes());
        Ok(())
    }
    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        self.0.push(v);
        Ok(())
    }
    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.0.push_nil();
        Ok(())
    }
    fn serialize_some<T: ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        self.0.push_serialize(value)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.0.push_nil();
        Ok(())
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        let mut s = LuaTableSerializer::begin(self.0, 1);
        SerializeStruct::serialize_field(&mut s, "__unit_struct", name)?;
        SerializeStruct::end(s)
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        let mut s = LuaTableSerializer::begin(self.0, 1);
        s.serialize_entry(&0, &variant_index);
        s.serialize_entry(variant, &true);
        SerializeMap::end(s)
    }
    fn serialize_newtype_struct<T: ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        self.0.push_serialize(value)
    }
    fn serialize_newtype_variant<T: ?Sized>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        let mut s = LuaTableSerializer::begin(self.0, 1);
        s.serialize_entry(&0, &variant_index);
        s.serialize_entry("__tag", variant);
        s.serialize_entry(variant, value);
        SerializeMap::end(s)
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(LuaTableSerializer::begin_array(self.0, len.unwrap_or(0)))
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(LuaTableSerializer::begin_array(self.0, len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(LuaTableSerializer::begin_array(self.0, len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        let mut s = LuaTableSerializer::begin_array(self.0, len);
        s.serialize_entry(&0, &variant_index);
        s.serialize_entry("__tag", variant);
        Ok(s)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(LuaTableSerializer::begin(self.0, len.unwrap_or(0)))
    }

    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(LuaTableSerializer::begin(self.0, len))
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        let mut s = LuaTableSerializer::begin(self.0, 1);
        s.serialize_entry(&0, &variant_index);
        s.serialize_entry("__tag", variant);
        Ok(s)
    }

    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> {
        Err(core::fmt::Error)
    }
    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> {
        Err(core::fmt::Error)
    }

    // fn collect_str<T: ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error>
    // where
    //     T: core::fmt::Display,
    // {
    //     Err(core::fmt::Error)
    // }
}

impl<'de> Deserializer<'de> for ValRef<'de> {
    type Error = DesErr;

    /// Require the `Deserializer` to figure out how to drive the visitor based
    /// on what data type is in the input.
    ///
    /// When implementing `Deserialize`, you should avoid relying on
    /// `Deserializer::deserialize_any` unless you need to be told by the
    /// Deserializer what type is in the input. Know that relying on
    /// `Deserializer::deserialize_any` means your data type will be able to
    /// deserialize from self-describing formats only, ruling out Bincode and
    /// many others.
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.state.type_of(self.index) {
            Type::Number => {
                if self.state.is_integer(self.index) {
                    visitor.visit_i64(self.state.to_integer(self.index))
                } else {
                    visitor.visit_f64(self.state.to_number(self.index))
                }
            }
            Type::String => self.deserialize_str(visitor),
            Type::Boolean => self.deserialize_bool(visitor),
            Type::Table => {
                if self.state.raw_len(self.index) > 0 {
                    self.deserialize_seq(visitor)
                } else {
                    self.deserialize_map(visitor)
                }
            }
            _ => visitor.visit_unit(),
        }
    }

    /// Hint that the `Deserialize` type is expecting a `bool` value.
    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_bool(self.state.to_bool(self.index))
    }

    /// Hint that the `Deserialize` type is expecting an `i8` value.
    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i8(self.state.arg(self.index).ok_or(DesErr::ExpectedInteger)?)
    }

    /// Hint that the `Deserialize` type is expecting an `i16` value.
    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i16(self.state.arg(self.index).ok_or(DesErr::ExpectedInteger)?)
    }

    /// Hint that the `Deserialize` type is expecting an `i32` value.
    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i32(self.state.arg(self.index).ok_or(DesErr::ExpectedInteger)?)
    }

    /// Hint that the `Deserialize` type is expecting an `i64` value.
    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i64(self.state.arg(self.index).ok_or(DesErr::ExpectedInteger)?)
    }

    /// Hint that the `Deserialize` type is expecting a `u8` value.
    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u8(self.state.arg(self.index).ok_or(DesErr::ExpectedInteger)?)
    }

    /// Hint that the `Deserialize` type is expecting a `u16` value.
    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u16(self.state.arg(self.index).ok_or(DesErr::ExpectedInteger)?)
    }

    /// Hint that the `Deserialize` type is expecting a `u32` value.
    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u32(self.state.arg(self.index).ok_or(DesErr::ExpectedInteger)?)
    }

    /// Hint that the `Deserialize` type is expecting a `u64` value.
    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u64(self.state.arg(self.index).ok_or(DesErr::ExpectedInteger)?)
    }

    /// Hint that the `Deserialize` type is expecting a `f32` value.
    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_f32(self.state.arg(self.index).ok_or(DesErr::ExpectedInteger)?)
    }

    /// Hint that the `Deserialize` type is expecting a `f64` value.
    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_f64(self.state.arg(self.index).ok_or(DesErr::ExpectedInteger)?)
    }

    /// Hint that the `Deserialize` type is expecting a `char` value.
    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.state.type_of(self.index) {
            Type::Number => visitor.visit_char(self.state.to_integer(self.index) as u8 as _),
            Type::String => {
                let s = self.state.to_str(self.index).unwrap();
                visitor.visit_char(
                    s.chars()
                        .next()
                        .ok_or(DesErr::Message("empty char".into()))?,
                )
            }
            _ => Err(DesErr::Message("invalid char type".into())),
        }
    }

    /// Hint that the `Deserialize` type is expecting a string value and does
    /// not benefit from taking ownership of buffered data owned by the
    /// `Deserializer`.
    ///
    /// If the `Visitor` would benefit from taking ownership of `String` data,
    /// indiciate this to the `Deserializer` by using `deserialize_string`
    /// instead.
    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_borrowed_str(
            self.state
                .to_str(self.index)
                .ok_or(DesErr::ExpectedString)?,
        )
    }

    /// Hint that the `Deserialize` type is expecting a string value and would
    /// benefit from taking ownership of buffered data owned by the
    /// `Deserializer`.
    ///
    /// If the `Visitor` would not benefit from taking ownership of `String`
    /// data, indicate that to the `Deserializer` by using `deserialize_str`
    /// instead.
    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    /// Hint that the `Deserialize` type is expecting a byte array and does not
    /// benefit from taking ownership of buffered data owned by the
    /// `Deserializer`.
    ///
    /// If the `Visitor` would benefit from taking ownership of `Vec<u8>` data,
    /// indicate this to the `Deserializer` by using `deserialize_byte_buf`
    /// instead.
    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_borrowed_bytes(
            self.state
                .to_bytes(self.index)
                .ok_or(DesErr::ExpectedString)?,
        )
    }

    /// Hint that the `Deserialize` type is expecting a byte array and would
    /// benefit from taking ownership of buffered data owned by the
    /// `Deserializer`.
    ///
    /// If the `Visitor` would not benefit from taking ownership of `Vec<u8>`
    /// data, indicate that to the `Deserializer` by using `deserialize_bytes`
    /// instead.
    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    /// Hint that the `Deserialize` type is expecting an optional value.
    ///
    /// This allows deserializers that encode an optional value as a nullable
    /// value to convert the null value into `None` and a regular value into
    /// `Some(value)`.
    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if self.state.is_none_or_nil(self.index) {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    /// Hint that the `Deserialize` type is expecting a unit value.
    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if self.state.is_none_or_nil(self.index) {
            visitor.visit_unit()
        } else {
            Err(DesErr::ExpectedNull)
        }
    }

    /// Hint that the `Deserialize` type is expecting a unit struct with a
    /// particular name.
    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    /// Hint that the `Deserialize` type is expecting a newtype struct with a
    /// particular name.
    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    /// Hint that the `Deserialize` type is expecting a sequence of values.
    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        struct SeqDes<'a>(ValRef<'a>, usize, usize);

        impl<'de> SeqAccess<'de> for SeqDes<'de> {
            type Error = DesErr;

            #[inline]
            fn size_hint(&self) -> Option<usize> {
                Some(self.0.state.raw_len(self.0.index))
            }

            fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
            where
                T: DeserializeSeed<'de>,
            {
                if self.1 > self.2 {
                    return Ok(None);
                }
                self.0.state.raw_geti(self.0.index, self.1 as _);
                self.1 += 1;
                let r = seed.deserialize(self.0.state.val(-1))?;
                self.0.state.pop(1);
                Ok(Some(r))
            }
        }

        if self.state.is_table(self.index) {
            visitor.visit_seq(SeqDes(self, 1, self.state.raw_len(self.index)))
        } else {
            Err(DesErr::ExpectedArray)
        }
    }

    /// Hint that the `Deserialize` type is expecting a sequence of values and
    /// knows how many values there are without looking at the serialized data.
    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    /// Hint that the `Deserialize` type is expecting a tuple struct with a
    /// particular name and number of fields.
    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    /// Hint that the `Deserialize` type is expecting a map of key-value pairs.
    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        impl<'de> MapAccess<'de> for ValRef<'de> {
            type Error = DesErr;

            fn next_key_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
            where
                T: DeserializeSeed<'de>,
            {
                if self.state.next(self.index) {
                    Ok(Some(seed.deserialize(self.state.val(-2))?))
                } else {
                    Ok(None)
                }
            }

            fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
            where
                V: DeserializeSeed<'de>,
            {
                let r = seed.deserialize(self.state.val(-1))?;
                self.state.pop(1);
                Ok(r)
            }
        }

        // crash if index is not a table
        if self.state.is_table(self.index) {
            self.state.push_nil();
            visitor.visit_map(self)
        } else {
            Err(DesErr::ExpectedMap)
        }
    }

    /// Hint that the `Deserialize` type is expecting a struct with a particular
    /// name and fields.
    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    /// Hint that the `Deserialize` type is expecting an enum value with a
    /// particular name and possible variants.
    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    /// Hint that the `Deserialize` type is expecting the name of a struct
    /// field or the discriminant of an enum variant.
    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    /// Hint that the `Deserialize` type needs to deserialize a value whose type
    /// doesn't matter because it is ignored.
    ///
    /// Deserializers for non-self-describing formats may not support this mode.
    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
}

struct DeLua<'a>(&'a State);

impl<'de> DeserializeSeed<'de> for DeLua<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(LuaVisitor(self.0))?;
        Ok(())
    }
}

pub struct LuaVisitor<'a>(pub &'a State);

impl<'de> Visitor<'de> for LuaVisitor<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("lua visitor")
    }

    fn visit_bool<E: DeErr>(self, v: bool) -> Result<Self::Value, E> {
        self.0.push(v);
        Ok(())
    }

    fn visit_i64<E: DeErr>(self, v: i64) -> Result<Self::Value, E> {
        self.0.push(v);
        Ok(())
    }

    fn visit_f64<E: DeErr>(self, v: f64) -> Result<Self::Value, E> {
        self.0.push(v);
        Ok(())
    }

    fn visit_u64<E: DeErr>(self, v: u64) -> Result<Self::Value, E> {
        self.0.push(v);
        Ok(())
    }

    fn visit_bytes<E: DeErr>(self, v: &[u8]) -> Result<Self::Value, E> {
        self.0.push(v);
        Ok(())
    }

    fn visit_str<E: DeErr>(self, v: &str) -> Result<Self::Value, E> {
        self.0.push(v);
        Ok(())
    }

    fn visit_none<E: DeErr>(self) -> Result<Self::Value, E> {
        self.0.push_nil();
        Ok(())
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(LuaVisitor(self.0))
    }

    fn visit_unit<E: DeErr>(self) -> Result<Self::Value, E> {
        self.0.push_nil();
        Ok(())
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        self.0
            .check_stack(3)
            .then_some(())
            .ok_or_else(|| A::Error::custom("stack not enough"))?;

        let size = seq.size_hint();
        self.0.create_table(size.unwrap_or_default() as _, 0);

        if let Some(size) = size {
            for i in 1..=size {
                match seq.next_element_seed(DeLua(self.0))? {
                    Some(_) => self.0.raw_seti(-2, i as lua_Integer),
                    None => continue,
                }
            }
        } else {
            let mut i = 0;
            while let Some(_) = seq.next_element_seed(DeLua(self.0))? {
                i += 1;
                self.0.raw_seti(-2, i);
            }
        }
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        self.0
            .check_stack(3)
            .then_some(())
            .ok_or_else(|| A::Error::custom("stack not enough"))?;

        let size = map.size_hint();
        self.0.create_table(0, size.unwrap_or_default() as _);

        if let Some(size) = size {
            for _ in 1..=size {
                match map.next_key_seed(DeLua(self.0))? {
                    Some(_) => {
                        map.next_value_seed(DeLua(self.0))?;
                        self.0.raw_set(-3);
                    }
                    None => continue,
                }
            }
        } else {
            while let Some(_) = map.next_key_seed(DeLua(self.0))? {
                map.next_value_seed(DeLua(self.0))?;
                self.0.raw_set(-3);
            }
        }

        Ok(())
    }
}

impl Serialize for ValRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        unsafe {
            match lua_type(self.state.as_ptr(), self.index) {
                LUA_TSTRING => {
                    let bytes = self.state.to_bytes(self.index).unwrap_or_default();
                    // TODO:
                    if bytes.len() > 0x1000 {
                        serializer.serialize_bytes(bytes)
                    } else {
                        match std::str::from_utf8(bytes) {
                            Ok(s) => serializer.serialize_str(s),
                            Err(_) => serializer.serialize_bytes(bytes),
                        }
                    }
                }
                // LUA_TSTRING => serializer.serialize_str(self.to_str(self.index).unwrap_or_default()),
                LUA_TNUMBER => {
                    if self.is_integer() {
                        serializer.serialize_i64(self.state.to_integer(self.index))
                    } else {
                        serializer.serialize_f64(self.state.to_number(self.index))
                    }
                }
                // TODO: serde option
                LUA_TFUNCTION => serializer.serialize_bool(true),
                LUA_TBOOLEAN => serializer.serialize_bool(self.to_bool()),
                LUA_TTABLE => {
                    let len = self.state.raw_len(self.index) as usize;
                    self.state
                        .check_stack(3)
                        .then_some(())
                        .ok_or_else(|| S::Error::custom("stack not enough"))?;
                    if len > 0 {
                        let mut seq = serializer.serialize_seq(Some(len))?;
                        for i in 1..=len {
                            self.state.raw_geti(self.index, i as lua_Integer);
                            let res = seq.serialize_element(&self.state.val(-1));
                            self.state.pop(1);
                            res?;
                        }
                        seq.end()
                    } else {
                        // get count of entries in the table
                        let mut count = 0usize;
                        self.state.push_nil();
                        while lua_next(self.state.as_ptr(), self.index) != 0 {
                            count += 1;
                            self.state.pop(1);
                        }
                        // serialize empty table as empty array
                        if count == 0 {
                            serializer.serialize_seq(Some(len))?.end()
                        } else {
                            let mut map = serializer.serialize_map(Some(count))?;
                            self.state.push_nil();
                            while lua_next(self.state.as_ptr(), self.index) != 0 {
                                let res =
                                    map.serialize_entry(&self.state.val(-2), &self.state.val(-1));
                                self.state.pop(1);
                                res?;
                            }
                            map.end()
                        }
                    }
                }
                _ => serializer.serialize_none(),
            }
        }
    }
}

impl Serialize for CRegVal<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.state.balance_with(|s| {
            s.push(self.creg_ref());
            s.val(-1).serialize(serializer)
        })
    }
}
