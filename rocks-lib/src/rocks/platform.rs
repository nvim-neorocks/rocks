use eyre::{eyre, Result};
use std::{collections::HashMap, str::FromStr};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, EnumString};

use serde::{Deserialize, Serialize};

#[derive(
    Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone, Display, EnumString, EnumIter,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum PlatformIdentifier {
    Unix,
    Windows,
    Win32,
    Cygwin,
    MacOSX,
    Linux,
    FreeBSD,
}

pub fn get_platform() -> PlatformIdentifier {
    if cfg!(linux) {
        PlatformIdentifier::Linux
    } else if cfg!(macos) {
        PlatformIdentifier::MacOSX
    } else if cfg!(cygwin) {
        PlatformIdentifier::Cygwin
    } else if cfg!(freebsd) {
        PlatformIdentifier::FreeBSD
    } else if cfg!(unix) {
        PlatformIdentifier::Unix
    } else if cfg!(target_os = "windows") && cfg!(target_arch = "x86") {
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
    fn get_subsets(&self) -> Vec<Self> {
        match self {
            PlatformIdentifier::Linux => vec![PlatformIdentifier::Unix],
            PlatformIdentifier::MacOSX => vec![PlatformIdentifier::Unix],
            PlatformIdentifier::Cygwin => vec![PlatformIdentifier::Unix],
            PlatformIdentifier::Windows => vec![PlatformIdentifier::Win32],
            PlatformIdentifier::FreeBSD => vec![PlatformIdentifier::Unix],
            PlatformIdentifier::Unix => vec![],
            PlatformIdentifier::Win32 => vec![],
        }
    }

    /// Get identifiers that are an extension of this identifier.
    /// For example, Linux is an extension of Unix
    fn get_extended_platforms(&self) -> Vec<Self> {
        match self {
            PlatformIdentifier::Linux => vec![],
            PlatformIdentifier::MacOSX => vec![],
            PlatformIdentifier::Cygwin => vec![],
            PlatformIdentifier::Windows => vec![],
            PlatformIdentifier::FreeBSD => vec![],
            PlatformIdentifier::Unix => vec![
                PlatformIdentifier::Linux,
                PlatformIdentifier::MacOSX,
                PlatformIdentifier::Cygwin,
                PlatformIdentifier::FreeBSD,
            ],
            PlatformIdentifier::Win32 => vec![PlatformIdentifier::Windows],
        }
    }

    #[cfg(test)]
    fn is_subset_of(&self, identifier: &PlatformIdentifier) -> bool {
        identifier.get_subsets().contains(self)
    }

    #[cfg(test)]
    fn is_extension_of(&self, identifier: &PlatformIdentifier) -> bool {
        identifier.get_extended_platforms().contains(self)
    }
}

#[derive(Debug, PartialEq)]
pub struct PlatformSupport {
    /// Do not match this platform
    platform_map: HashMap<PlatformIdentifier, bool>,
}

impl Default for PlatformSupport {
    fn default() -> Self {
        let mut platform_map = HashMap::default();
        for identifier in PlatformIdentifier::iter() {
            platform_map.insert(identifier, true);
        }
        return Self { platform_map };
    }
}

impl PlatformSupport {
    pub fn new(platforms: &Vec<String>) -> Result<Self> {
        let mut platform_map = HashMap::default();
        if platforms.is_empty() {
            return Ok(Self::default());
        }
        let mut only_negative_entries = true;
        for raw_str in platforms {
            let (is_supported, platform_str) = if raw_str.starts_with("!") {
                // trim leading "!"
                let trimmed = &raw_str[1..raw_str.len()];
                (false, trimmed)
            } else {
                only_negative_entries = false;
                (true, raw_str.as_str())
            };
            let platform_identifier = PlatformIdentifier::from_str(platform_str)?;
            if *platform_map
                .get(&platform_identifier)
                .unwrap_or(&is_supported)
                != is_supported
            {
                return Err(eyre!("Conflicting supported_platforms entries!"));
            }
            platform_map.insert(platform_identifier.clone(), is_supported);
            if is_supported {
                // Supported extends to extended platforms
                for sub_platform in platform_identifier.get_extended_platforms() {
                    if *platform_map.get(&sub_platform).unwrap_or(&is_supported) != is_supported {
                        return Err(eyre!("Conflicting supported_platforms entries!"));
                    }
                    platform_map.insert(sub_platform, is_supported);
                }
            } else {
                // Unsupported extends to subset platforms
                for sub_platform in platform_identifier.get_subsets() {
                    if *platform_map.get(&sub_platform).unwrap_or(&is_supported) != is_supported {
                        return Err(eyre!("Conflicting supported_platforms entries!"));
                    }
                    platform_map.insert(sub_platform, is_supported);
                }
            }
        }
        // Special case
        if only_negative_entries {
            // Support any platform except those listed.
            for identifier in PlatformIdentifier::iter() {
                if *platform_map.get(&identifier).unwrap_or(&true) {
                    platform_map.insert(identifier, true);
                }
            }
        }
        Ok(Self { platform_map })
    }

    pub fn is_supported(&self, platform: &PlatformIdentifier) -> bool {
        *self.platform_map.get(platform).unwrap_or(&false)
    }

    pub fn is_current_platform_supported(&self) -> bool {
        self.is_supported(&get_platform())
    }
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

    proptest! {
        #[test]
        fn supported_platforms(identifier in platform_identifier_strategy()) {
            let identifier_str = identifier.to_string();
            let platforms = vec![identifier_str];
            let platform_support = PlatformSupport::new(&platforms).unwrap();
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
            let platform_support = PlatformSupport::new(&platforms).unwrap();
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
            let platform_support = PlatformSupport::new(&platforms).unwrap();
            prop_assert!(platform_support.is_supported(&unspecified));
            prop_assert!(!platform_support.is_supported(&unsupported));
        }

        #[test]
        fn all_platforms_supported_if_none_are_specified(identifier in platform_identifier_strategy()) {
            let platforms = vec![];
            let platform_support = PlatformSupport::new(&platforms).unwrap();
            prop_assert!(platform_support.is_supported(&identifier))
        }

        #[test]
        fn conflicting_platforms(identifier in platform_identifier_strategy()) {
            let identifier_str = identifier.to_string();
            let identifier_str_negated = format!("!{}", identifier);
            let platforms = vec![identifier_str, identifier_str_negated];
            let _ = PlatformSupport::new(&platforms).unwrap_err();
        }

        #[test]
        fn extended_platforms_supported_if_supported(identifier in platform_identifier_strategy()) {
            let identifier_str = identifier.to_string();
            let platforms = vec![identifier_str];
            let platform_support = PlatformSupport::new(&platforms).unwrap();
            for identifier in identifier.get_extended_platforms() {
                prop_assert!(platform_support.is_supported(&identifier))
            }
        }

        #[test]
        fn sub_platforms_unsupported_if_unsupported(identifier in platform_identifier_strategy()) {
            let identifier_str = format!("!{}", identifier);
            let platforms = vec![identifier_str];
            let platform_support = PlatformSupport::new(&platforms).unwrap();
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
            let _ = PlatformSupport::new(&platforms).unwrap_err();
        }
    }
}
