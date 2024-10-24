use std::{collections::HashMap, convert::Infallible};

use serde::Deserialize;

use super::{PartialOverride, PerPlatform, PlatformOverridable};

/// Can be defined in a [platform-agnostic](https://github.com/luarocks/luarocks/wiki/platform-agnostic-external-dependencies) manner
#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ExternalDependency {
    /// A header file, e.g. "foo.h"
    Header(String),
    /// A library file, e.g. "foo.lib"
    Library(String),
}

impl PartialOverride for HashMap<String, ExternalDependency> {
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

impl PlatformOverridable for HashMap<String, ExternalDependency> {
    type Err = Infallible;

    fn on_nil<T>() -> Result<super::PerPlatform<T>, <Self as PlatformOverridable>::Err>
    where
        T: PlatformOverridable,
        T: Default,
    {
        Ok(PerPlatform::default())
    }
}
