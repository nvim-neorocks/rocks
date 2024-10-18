use eyre::{OptionExt, Result};
use itertools::Itertools as _;
use serde::{de, Deserialize, Deserializer};

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

/// Convert a json value into a Vec<T>, treating empty json objects as empty lists
/// This is needed to be able to deserialise Lua tables.
pub fn mlua_json_value_to_vec<T>(values: serde_json::Value) -> Result<Vec<T>>
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
        .ok_or_eyre("expected a list of strings")?
        .iter()
        .map(|val| {
            let str: String = val
                .as_str()
                .map(|s| s.into())
                .ok_or_eyre("expected a list of strings")?;
            Ok(str.into())
        })
        .try_collect()
}
