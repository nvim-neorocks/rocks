use serde::{de::IntoDeserializer, Deserialize, Deserializer};

#[derive(Debug, PartialEq, Deserialize, Default)]
pub struct BuildSpec {
    #[serde(rename = "type", default)]
    pub build_type: BuildType,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase", remote = "BuildType")]
pub enum BuildType {
    /// "builtin" or "module"
    Builtin,
    /// "make"
    Make,
    /// "cmake"
    CMake,
    /// "command"
    Command,
    /// "none"
    None,
    /// "cargo" (rust)
    Cargo,
    /// external Lua rock
    LuaRock(String),
}

// Special Deserialize case for BuildType:
// Both "module" and "builtin" map to `Builtin`
impl<'de> Deserialize<'de> for BuildType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == "builtin" || s == "module" {
            Ok(Self::Builtin)
        } else {
            match Self::deserialize(s.clone().into_deserializer()) {
                Err(_) => Ok(Self::LuaRock(s)),
                ok => ok,
            }
        }
    }
}

impl Default for BuildType {
    fn default() -> Self {
        Self::Builtin
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    pub async fn deserialize_build_type() {
        let build_type: BuildType = serde_json::from_str("\"builtin\"".into()).unwrap();
        assert_eq!(build_type, BuildType::Builtin);
        let build_type: BuildType = serde_json::from_str("\"module\"".into()).unwrap();
        assert_eq!(build_type, BuildType::Builtin);
        let build_type: BuildType = serde_json::from_str("\"make\"".into()).unwrap();
        assert_eq!(build_type, BuildType::Make);
        let build_type: BuildType =
            serde_json::from_str("\"luarocks_build_rust_mlua\"".into()).unwrap();
        assert_eq!(
            build_type,
            BuildType::LuaRock("luarocks_build_rust_mlua".into())
        );
    }
}
