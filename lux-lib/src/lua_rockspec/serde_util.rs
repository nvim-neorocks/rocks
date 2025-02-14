use std::fmt::Display;

use itertools::Itertools;
use serde::{de, Deserialize, Deserializer};
use thiserror::Error;

#[derive(Hash, Debug, Eq, PartialEq, Clone)]
pub enum LuaTableKey {
    IntKey(u64),
    StringKey(String),
}

impl<'de> Deserialize<'de> for LuaTableKey {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if value.is_u64() {
            Ok(LuaTableKey::IntKey(value.as_u64().unwrap()))
        } else if value.is_string() {
            Ok(LuaTableKey::StringKey(value.as_str().unwrap().into()))
        } else {
            Err(de::Error::custom(format!(
                "Could not parse Lua table key. Expected an integer or string, but got {}",
                value
            )))
        }
    }
}

/// Deserialize a json value into a Vec<T>, treating empty json objects as empty lists
/// This is needed to be able to deserialise Lua tables.
pub fn deserialize_vec_from_lua<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: From<String>,
{
    let values = serde_json::Value::deserialize(deserializer)?;
    mlua_json_value_to_vec(values).map_err(de::Error::custom)
}

#[derive(Error, Debug)]
#[error("expected list of strings")]
pub struct ExpectedListOfStrings;

/// Convert a json value into a Vec<T>, treating empty json objects as empty lists
/// This is needed to be able to deserialise Lua tables.
pub fn mlua_json_value_to_vec<T>(values: serde_json::Value) -> Result<Vec<T>, ExpectedListOfStrings>
where
    T: From<String>,
{
    // If we deserialise an empty Lua table, mlua treats it as a dictionary.
    // This case is handled here.
    if let Some(values_as_obj) = values.as_object() {
        if values_as_obj.is_empty() {
            return Ok(Vec::default());
        }
    }
    values
        .as_array()
        .ok_or(ExpectedListOfStrings)?
        .iter()
        .map(|val| {
            let str: String = val
                .as_str()
                .map(|s| s.into())
                .ok_or(ExpectedListOfStrings)?;
            Ok(str.into())
        })
        .try_collect()
}

pub(crate) enum DisplayLuaValue {
    // NOTE(vhyrro): these are not used in the current implementation
    // Nil,
    // Number(f64),
    Boolean(bool),
    String(String),
    List(Vec<Self>),
    Table(Vec<DisplayLuaKV>),
}

pub(crate) struct DisplayLuaKV {
    pub(crate) key: String,
    pub(crate) value: DisplayLuaValue,
}

impl Display for DisplayLuaValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            //DisplayLuaValue::Nil => write!(f, "nil"),
            //DisplayLuaValue::Number(n) => write!(f, "{}", n),
            DisplayLuaValue::Boolean(b) => write!(f, "{}", b),
            DisplayLuaValue::String(s) => write!(f, "\"{}\"", s),
            DisplayLuaValue::List(l) => {
                writeln!(f, "{{")?;

                for item in l {
                    writeln!(f, "{},", item)?;
                }

                write!(f, "}}")?;

                Ok(())
            }
            DisplayLuaValue::Table(t) => {
                writeln!(f, "{{")?;

                for item in t {
                    writeln!(f, "{},", item)?;
                }

                write!(f, "}}")?;

                Ok(())
            }
        }
    }
}

impl Display for DisplayLuaKV {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} = {}", self.key, self.value)
    }
}

/// Trait for serializing a Lua structure from a rockspec into a `key = value` pair.
pub(crate) trait DisplayAsLuaKV {
    fn display_lua(&self) -> DisplayLuaKV;
}

pub(crate) trait DisplayAsLuaValue {
    fn display_lua_value(&self) -> DisplayLuaValue;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_lua_value() {
        let value = DisplayLuaValue::String("hello".to_string());
        assert_eq!(format!("{}", value), "\"hello\"");

        let value = DisplayLuaValue::Boolean(true);
        assert_eq!(format!("{}", value), "true");

        let value = DisplayLuaValue::List(vec![
            DisplayLuaValue::String("hello".to_string()),
            DisplayLuaValue::Boolean(true),
        ]);
        assert_eq!(format!("{}", value), "{\n\"hello\",\ntrue,\n}");

        let value = DisplayLuaValue::Table(vec![
            DisplayLuaKV {
                key: "key".to_string(),
                value: DisplayLuaValue::String("value".to_string()),
            },
            DisplayLuaKV {
                key: "key2".to_string(),
                value: DisplayLuaValue::Boolean(true),
            },
        ]);
        assert_eq!(format!("{}", value), "{\nkey = \"value\",\nkey2 = true,\n}");
    }
}
