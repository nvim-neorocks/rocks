use eyre::{eyre, Result};
use itertools::Itertools;
use mlua::{FromLua, Lua, LuaSerdeExt as _, Value};
use std::{cmp::Ordering, collections::HashMap, marker::PhantomData};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, EnumString};

use serde::{
    de::{self, DeserializeOwned},
    Deserialize, Deserializer, Serialize,
};

use crate::remote_package::PackageReq;

/// Identifier by a platform.
/// The `PartialOrd` instance views more specific platforms as `Greater`
#[derive(
    Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Copy, Clone, Display, EnumString, EnumIter,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum PlatformIdentifier {
    // TODO: Add undocumented platform identifiers from luarocks codebase?
    Unix,
    Windows,
    Win32,
    Cygwin,
    MacOSX,
    Linux,
    FreeBSD,
}

// Order by specificity -> less specific = `Less`
impl PartialOrd for PlatformIdentifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (PlatformIdentifier::Unix, PlatformIdentifier::Cygwin) => Some(Ordering::Less),
            (PlatformIdentifier::Unix, PlatformIdentifier::MacOSX) => Some(Ordering::Less),
            (PlatformIdentifier::Unix, PlatformIdentifier::Linux) => Some(Ordering::Less),
            (PlatformIdentifier::Unix, PlatformIdentifier::FreeBSD) => Some(Ordering::Less),
            (PlatformIdentifier::Windows, PlatformIdentifier::Win32) => Some(Ordering::Greater),
            (PlatformIdentifier::Win32, PlatformIdentifier::Windows) => Some(Ordering::Less),
            (PlatformIdentifier::Cygwin, PlatformIdentifier::Unix) => Some(Ordering::Greater),
            (PlatformIdentifier::MacOSX, PlatformIdentifier::Unix) => Some(Ordering::Greater),
            (PlatformIdentifier::Linux, PlatformIdentifier::Unix) => Some(Ordering::Greater),
            (PlatformIdentifier::FreeBSD, PlatformIdentifier::Unix) => Some(Ordering::Greater),
            _ if self == other => Some(Ordering::Equal),
            _ => None,
        }
    }
}

/// Retrieves the target compilation platform and returns it as an identifier.
pub fn get_platform() -> PlatformIdentifier {
    if cfg!(target_os = "linux") {
        PlatformIdentifier::Linux
    } else if cfg!(target_os = "macos") {
        PlatformIdentifier::MacOSX
    } else if cfg!(target_os = "freebsd") {
        PlatformIdentifier::FreeBSD
    } else if which::which("cygpath").is_ok() {
        PlatformIdentifier::Cygwin
    } else if cfg!(unix) {
        PlatformIdentifier::Unix
    } else if cfg!(all(target_os = "windows", target_arch = "x86")) {
        PlatformIdentifier::Win32
    } else if cfg!(windows) {
        PlatformIdentifier::Windows
    } else {
        panic!("Could not determine the platform.")
    }
}

impl PlatformIdentifier {
    /// Get identifiers that are a subset of this identifier.
    /// For example, Unix is a subset of Linux
    pub fn get_subsets(&self) -> Vec<Self> {
        PlatformIdentifier::iter()
            .filter(|identifier| identifier.is_subset_of(self))
            .collect()
    }

    /// Get identifiers that are an extension of this identifier.
    /// For example, Linux is an extension of Unix
    pub fn get_extended_platforms(&self) -> Vec<Self> {
        PlatformIdentifier::iter()
            .filter(|identifier| identifier.is_extension_of(self))
            .collect()
    }

    /// e.g. Unix is a subset of Linux
    fn is_subset_of(&self, other: &PlatformIdentifier) -> bool {
        self.partial_cmp(other) == Some(Ordering::Less)
    }

    /// e.g. Linux is an extension of Unix
    fn is_extension_of(&self, other: &PlatformIdentifier) -> bool {
        self.partial_cmp(other) == Some(Ordering::Greater)
    }
}

#[derive(Debug, PartialEq)]
pub struct PlatformSupport {
    /// Do not match this platform
    platform_map: HashMap<PlatformIdentifier, bool>,
}

impl Default for PlatformSupport {
    fn default() -> Self {
        Self {
            platform_map: PlatformIdentifier::iter()
                .map(|identifier| (identifier, true))
                .collect(),
        }
    }
}

impl<'de> Deserialize<'de> for PlatformSupport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let platforms: Vec<String> = Vec::deserialize(deserializer)?;
        Self::parse(&platforms).map_err(de::Error::custom)
    }
}

impl PlatformSupport {
    fn validate_platforms(platforms: &[String]) -> Result<HashMap<PlatformIdentifier, bool>> {
        platforms
            .iter()
            .try_fold(HashMap::new(), |mut platforms, platform| {
                // Platform assertions can exist in one of the following forms:
                // - `platform` - a positive assertion for the platform (the platform must be present)
                // - `!platform` - a negative assertion for the platform (any platform *but* this one must be present)
                let (is_positive_assertion, platform) = platform
                    .strip_prefix('!')
                    .map(|str| (false, str))
                    .unwrap_or((true, platform));

                let platform_identifier: PlatformIdentifier = platform.parse()?;

                // If a platform with the same name exists already and is contradictory
                // then throw an error. An example of such a contradiction is e.g.:
                // [`win32`, `!win32`]
                if platforms
                    .get(&platform_identifier)
                    .unwrap_or(&is_positive_assertion)
                    != &is_positive_assertion
                {
                    return Err(eyre!("Conflicting supported platform entries!"));
                }

                platforms.insert(platform_identifier, is_positive_assertion);

                let subset_or_extended_platforms = if is_positive_assertion {
                    platform_identifier.get_extended_platforms()
                } else {
                    platform_identifier.get_subsets()
                };

                for sub_platform in subset_or_extended_platforms {
                    if platforms
                        .get(&sub_platform)
                        .unwrap_or(&is_positive_assertion)
                        != &is_positive_assertion
                    {
                        // TODO(vhyrro): More detailed errors
                        return Err(eyre!("Conflicting supported platform entries!"));
                    }

                    platforms.insert(sub_platform, is_positive_assertion);
                }

                Ok(platforms)
            })
    }

    pub fn parse(platforms: &[String]) -> Result<Self> {
        match platforms {
            [] => Ok(Self::default()),
            platforms if platforms.iter().all(|platform| platform.starts_with('!')) => {
                let mut platform_map = Self::validate_platforms(platforms)?;

                for identifier in PlatformIdentifier::iter() {
                    platform_map.entry(identifier).or_insert(true);
                }

                Ok(Self { platform_map })
            }
            _ => Ok(Self {
                platform_map: Self::validate_platforms(platforms)?,
            }),
        }
    }

    pub fn is_supported(&self, platform: &PlatformIdentifier) -> bool {
        self.platform_map.get(platform).cloned().unwrap_or(false)
    }

    pub fn is_current_platform_supported(&self) -> bool {
        self.is_supported(&get_platform())
    }
}

pub trait PartialOverride: Sized {
    fn apply_overrides(&self, override_val: &Self) -> Result<Self>;
}

/// Override `base_deps` with `override_deps`
/// - Adds missing dependencies
/// - Replaces dependencies with the same name
impl PartialOverride for Vec<PackageReq> {
    fn apply_overrides(&self, override_vec: &Self) -> Result<Self> {
        let mut result_map: HashMap<String, PackageReq> = self
            .iter()
            .map(|dep| (dep.name().clone().to_string(), dep.clone()))
            .collect();
        for override_dep in override_vec {
            result_map.insert(
                override_dep.name().clone().to_string(),
                override_dep.clone(),
            );
        }
        Ok(result_map.into_values().collect())
    }
}

pub trait PlatformOverridable: PartialOverride {
    fn on_nil<T>() -> Result<PerPlatform<T>>
    where
        T: PlatformOverridable,
        T: Default;
}

impl PlatformOverridable for Vec<PackageReq> {
    fn on_nil<T>() -> Result<super::PerPlatform<T>>
    where
        T: PlatformOverridable,
        T: Default,
    {
        Ok(PerPlatform::default())
    }
}

pub trait FromPlatformOverridable<T: PlatformOverridable, G: FromPlatformOverridable<T, G>> {
    fn from_platform_overridable(internal: T) -> Result<G>;
}

/// Data that that can vary per platform
#[derive(Debug, PartialEq)]
pub struct PerPlatform<T> {
    /// The base data, applicable if no platform is specified
    pub default: T,
    /// The per-platform override, if present.
    pub per_platform: HashMap<PlatformIdentifier, T>,
}

impl<T> PerPlatform<T> {
    pub fn get(&self, platform: &PlatformIdentifier) -> &T {
        self.per_platform.get(platform).unwrap_or(
            platform
                .get_subsets()
                .into_iter()
                // More specific platforms first.
                // This is safe because a platform's subsets
                // can be totally ordered among each other.
                .sorted_by(|a, b| b.partial_cmp(a).unwrap_or(Ordering::Equal))
                .find(|identifier| self.per_platform.contains_key(identifier))
                .and_then(|identifier| self.per_platform.get(&identifier))
                .unwrap_or(&self.default),
        )
    }

    pub fn current_platform(&self) -> &T {
        self.get(&get_platform())
    }
}

impl<T: Default> Default for PerPlatform<T> {
    fn default() -> Self {
        Self {
            default: T::default(),
            per_platform: HashMap::default(),
        }
    }
}

impl<'lua, T> FromLua<'lua> for PerPlatform<T>
where
    T: PlatformOverridable,
    T: DeserializeOwned,
    T: Default,
    T: Clone,
{
    fn from_lua(value: Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
        match &value {
            list @ Value::Table(tbl) => {
                let mut per_platform = match tbl.get("platforms")? {
                    val @ Value::Table(_) => Ok(lua.from_value(val)?),
                    Value::Nil => Ok(HashMap::default()),
                    val => Err(mlua::Error::DeserializeError(format!(
                        "Expected platforms to be a table or nil, but got {}",
                        val.type_name()
                    ))),
                }?;
                let _ = tbl.raw_remove("platforms");
                let default = lua.from_value(list.to_owned())?;
                apply_per_platform_overrides(&mut per_platform, &default)
                    .map_err(|err| mlua::Error::DeserializeError(err.to_string()))?;
                Ok(PerPlatform {
                    default,
                    per_platform,
                })
            }
            Value::Nil => T::on_nil().map_err(|err| mlua::Error::DeserializeError(err.to_string())),
            val => Err(mlua::Error::DeserializeError(format!(
                "Expected rockspec external dependencies to be a table or nil, but got {}",
                val.type_name()
            ))),
        }
    }
}

/// Newtype wrapper used to implement a `FromLua` instance for `FromPlatformOverridable`
/// This is necessary, because Rust doesn't yet support specialization.
pub struct PerPlatformWrapper<T, G> {
    pub un_per_platform: PerPlatform<T>,
    phantom: PhantomData<G>,
}

impl<'lua, T, G> FromLua<'lua> for PerPlatformWrapper<T, G>
where
    T: FromPlatformOverridable<G, T>,
    G: PlatformOverridable,
    G: DeserializeOwned,
    G: Default,
    G: Clone,
{
    fn from_lua(value: Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
        let internal = PerPlatform::from_lua(value, lua)?;
        let per_platform: HashMap<_, _> = internal
            .per_platform
            .into_iter()
            .map(|(platform, internal_override)| {
                let override_spec = T::from_platform_overridable(internal_override)
                    .map_err(|err| mlua::Error::DeserializeError(err.to_string()))?;

                Ok((platform, override_spec))
            })
            .try_collect::<_, _, mlua::Error>()?;
        let un_per_platform = PerPlatform {
            default: T::from_platform_overridable(internal.default)
                .map_err(|err| mlua::Error::DeserializeError(err.to_string()))?,
            per_platform,
        };
        Ok(PerPlatformWrapper {
            un_per_platform,
            phantom: PhantomData,
        })
    }
}

fn apply_per_platform_overrides<T>(
    per_platform: &mut HashMap<PlatformIdentifier, T>,
    base: &T,
) -> Result<()>
where
    T: PartialOverride,
    T: Default,
    T: Clone,
{
    let per_platform_raw = per_platform.clone();
    for (platform, overrides) in per_platform.clone() {
        // Add base values for each platform
        let overridden = base.apply_overrides(&overrides)?;
        per_platform.insert(platform, overridden);
    }
    for (platform, overrides) in per_platform_raw {
        // Add extended platform dependencies (without base deps) for each platform
        for extended_platform in &platform.get_extended_platforms() {
            let extended_overrides = per_platform
                .get(extended_platform)
                .cloned()
                .unwrap_or_default();
            per_platform.insert(
                *extended_platform,
                extended_overrides.apply_overrides(&overrides)?,
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    use super::*;
    use proptest::prelude::*;

    fn platform_identifier_strategy() -> impl Strategy<Value = PlatformIdentifier> {
        prop_oneof![
            Just(PlatformIdentifier::Unix),
            Just(PlatformIdentifier::Windows),
            Just(PlatformIdentifier::Win32),
            Just(PlatformIdentifier::Cygwin),
            Just(PlatformIdentifier::MacOSX),
            Just(PlatformIdentifier::Linux),
            Just(PlatformIdentifier::FreeBSD),
        ]
    }

    #[tokio::test]
    async fn sort_platform_identifier_more_specific_last() {
        let mut platforms = vec![
            PlatformIdentifier::Cygwin,
            PlatformIdentifier::Linux,
            PlatformIdentifier::Unix,
        ];
        platforms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        assert_eq!(
            platforms,
            vec![
                PlatformIdentifier::Unix,
                PlatformIdentifier::Cygwin,
                PlatformIdentifier::Linux
            ]
        );
        let mut platforms = vec![PlatformIdentifier::Windows, PlatformIdentifier::Win32];
        platforms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        assert_eq!(
            platforms,
            vec![PlatformIdentifier::Win32, PlatformIdentifier::Windows]
        )
    }

    #[tokio::test]
    async fn test_is_subset_of() {
        assert!(PlatformIdentifier::Unix.is_subset_of(&PlatformIdentifier::Linux));
        assert!(PlatformIdentifier::Unix.is_subset_of(&PlatformIdentifier::MacOSX));
        assert!(!PlatformIdentifier::Linux.is_subset_of(&PlatformIdentifier::Unix));
    }

    #[tokio::test]
    async fn test_is_extension_of() {
        assert!(PlatformIdentifier::Linux.is_extension_of(&PlatformIdentifier::Unix));
        assert!(PlatformIdentifier::MacOSX.is_extension_of(&PlatformIdentifier::Unix));
        assert!(!PlatformIdentifier::Unix.is_extension_of(&PlatformIdentifier::Linux));
    }

    #[tokio::test]
    async fn per_platform() {
        let foo = PerPlatform {
            default: "default",
            per_platform: vec![
                (PlatformIdentifier::Unix, "unix"),
                (PlatformIdentifier::FreeBSD, "freebsd"),
                (PlatformIdentifier::Cygwin, "cygwin"),
                (PlatformIdentifier::Linux, "linux"),
            ]
            .into_iter()
            .collect(),
        };
        assert_eq!(*foo.get(&PlatformIdentifier::MacOSX), "unix");
        assert_eq!(*foo.get(&PlatformIdentifier::Linux), "linux");
        assert_eq!(*foo.get(&PlatformIdentifier::FreeBSD), "freebsd");
        assert_eq!(*foo.get(&PlatformIdentifier::Cygwin), "cygwin");
        assert_eq!(*foo.get(&PlatformIdentifier::Windows), "default");
    }

    #[tokio::test]
    async fn test_override_lua_package_req() {
        let neorg_a: PackageReq = "neorg 1.0.0".parse().unwrap();
        let neorg_b: PackageReq = "neorg 2.0.0".parse().unwrap();
        let foo: PackageReq = "foo 1.0.0".parse().unwrap();
        let bar: PackageReq = "bar 1.0.0".parse().unwrap();
        let base_vec = vec![neorg_a, foo.clone()];
        let override_vec = vec![neorg_b.clone(), bar.clone()];
        let result = base_vec.apply_overrides(&override_vec).unwrap();
        assert_eq!(result.clone().len(), 3);
        assert_eq!(
            result
                .into_iter()
                .filter(|dep| *dep == neorg_b || *dep == foo || *dep == bar)
                .count(),
            3
        );
    }

    proptest! {
        #[test]
        fn supported_platforms(identifier in platform_identifier_strategy()) {
            let identifier_str = identifier.to_string();
            let platforms = vec![identifier_str];
            let platform_support = PlatformSupport::parse(&platforms).unwrap();
            prop_assert!(platform_support.is_supported(&identifier))
        }

        #[test]
        fn unsupported_platforms_only(unsupported in platform_identifier_strategy(), supported in platform_identifier_strategy()) {
            if supported == unsupported
                || unsupported.is_extension_of(&supported) {
                return Ok(());
            }
            let identifier_str = format!("!{}", unsupported);
            let platforms = vec![identifier_str];
            let platform_support = PlatformSupport::parse(&platforms).unwrap();
            prop_assert!(!platform_support.is_supported(&unsupported));
            prop_assert!(platform_support.is_supported(&supported))
        }

        #[test]
        fn supported_and_unsupported_platforms(unsupported in platform_identifier_strategy(), unspecified in platform_identifier_strategy()) {
            if unspecified == unsupported
                || unsupported.is_extension_of(&unspecified) {
                return Ok(());
            }
            let supported_str = unspecified.to_string();
            let unsupported_str = format!("!{}", unsupported);
            let platforms = vec![supported_str, unsupported_str];
            let platform_support = PlatformSupport::parse(&platforms).unwrap();
            prop_assert!(platform_support.is_supported(&unspecified));
            prop_assert!(!platform_support.is_supported(&unsupported));
        }

        #[test]
        fn all_platforms_supported_if_none_are_specified(identifier in platform_identifier_strategy()) {
            let platforms = vec![];
            let platform_support = PlatformSupport::parse(&platforms).unwrap();
            prop_assert!(platform_support.is_supported(&identifier))
        }

        #[test]
        fn conflicting_platforms(identifier in platform_identifier_strategy()) {
            let identifier_str = identifier.to_string();
            let identifier_str_negated = format!("!{}", identifier);
            let platforms = vec![identifier_str, identifier_str_negated];
            let _ = PlatformSupport::parse(&platforms).unwrap_err();
        }

        #[test]
        fn extended_platforms_supported_if_supported(identifier in platform_identifier_strategy()) {
            let identifier_str = identifier.to_string();
            let platforms = vec![identifier_str];
            let platform_support = PlatformSupport::parse(&platforms).unwrap();
            for identifier in identifier.get_extended_platforms() {
                prop_assert!(platform_support.is_supported(&identifier))
            }
        }

        #[test]
        fn sub_platforms_unsupported_if_unsupported(identifier in platform_identifier_strategy()) {
            let identifier_str = format!("!{}", identifier);
            let platforms = vec![identifier_str];
            let platform_support = PlatformSupport::parse(&platforms).unwrap();
            for identifier in identifier.get_subsets() {
                prop_assert!(!platform_support.is_supported(&identifier))
            }
        }

        #[test]
        fn conflicting_extended_platform_definitions(identifier in platform_identifier_strategy()) {
            let extended_platforms = identifier.get_extended_platforms();
            if extended_platforms.is_empty() {
                return Ok(());
            }
            let supported_str = identifier.to_string();
            let mut platforms: Vec<String> = extended_platforms.into_iter().map(|ident| format!("!{}", ident)).collect();
            platforms.push(supported_str);
            let _ = PlatformSupport::parse(&platforms).unwrap_err();
        }
    }
}
