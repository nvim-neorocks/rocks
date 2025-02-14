use std::{collections::HashMap, convert::Infallible, path::PathBuf};

use serde::Deserialize;
use thiserror::Error;

use super::{
    DisplayAsLuaKV, DisplayLuaKV, DisplayLuaValue, PartialOverride, PerPlatform,
    PlatformOverridable,
};

/// Can be defined in a [platform-agnostic](https://github.com/luarocks/luarocks/wiki/platform-agnostic-external-dependencies) manner
#[derive(Debug, PartialEq, Clone)]
pub enum ExternalDependencySpec {
    /// A header file, e.g. "foo.h"
    Header(PathBuf),
    /// A library file, e.g. "libfoo.so"
    Library(PathBuf),
}

#[derive(Error, Debug)]
#[error("conflicting external dependency specification")]
pub struct ConflictingExternalDependencySpec;

#[derive(Error, Debug)]
#[error("invalid external dependency key: {}", ._0)]
pub struct InvalidExternalDependencyKey(String);

impl<'de> Deserialize<'de> for ExternalDependencySpec {
    fn deserialize<D>(deserializer: D) -> Result<ExternalDependencySpec, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map: HashMap<String, PathBuf> = HashMap::deserialize(deserializer)?;

        if map.contains_key("header") == map.contains_key("library") {
            return Err(serde::de::Error::custom(ConflictingExternalDependencySpec));
        }

        match map.into_iter().next() {
            Some((key, value)) => match key.as_str() {
                "header" => Ok(ExternalDependencySpec::Header(value)),
                "library" => Ok(ExternalDependencySpec::Library(value)),
                key => Err(serde::de::Error::custom(InvalidExternalDependencyKey(
                    key.to_string(),
                ))),
            },
            None => unreachable!(),
        }
    }
}

impl PartialOverride for HashMap<String, ExternalDependencySpec> {
    type Err = Infallible;

    fn apply_overrides(&self, override_map: &Self) -> Result<Self, Self::Err> {
        let mut result = Self::new();
        for (key, value) in self {
            result.insert(key.clone(), value.clone());
        }
        for (key, value) in override_map {
            result.insert(key.clone(), value.clone());
        }
        Ok(result)
    }
}

impl PlatformOverridable for HashMap<String, ExternalDependencySpec> {
    type Err = Infallible;

    fn on_nil<T>() -> Result<super::PerPlatform<T>, <Self as PlatformOverridable>::Err>
    where
        T: PlatformOverridable,
        T: Default,
    {
        Ok(PerPlatform::default())
    }
}

pub(crate) struct ExternalDependencies<'a>(pub(crate) &'a HashMap<String, ExternalDependencySpec>);

impl DisplayAsLuaKV for ExternalDependencies<'_> {
    fn display_lua(&self) -> DisplayLuaKV {
        DisplayLuaKV {
            key: "external_dependencies".to_string(),
            value: DisplayLuaValue::Table(
                self.0
                    .iter()
                    .map(|(key, value)| DisplayLuaKV {
                        key: key.clone(),
                        value: DisplayLuaValue::Table(match value {
                            ExternalDependencySpec::Header(path) => vec![DisplayLuaKV {
                                key: "header".to_string(),
                                value: DisplayLuaValue::String(path.to_string_lossy().to_string()),
                            }],
                            ExternalDependencySpec::Library(path) => vec![DisplayLuaKV {
                                key: "library".to_string(),
                                value: DisplayLuaValue::String(path.to_string_lossy().to_string()),
                            }],
                        }),
                    })
                    .collect(),
            ),
        }
    }
}
