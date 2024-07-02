use semver::{Error, Version};

mod version;

pub use version::{parse_version, parse_version_req};

pub struct LuaRock {
    pub name: String,
    pub version: Version,
}

impl LuaRock {
    pub fn new(name: String, version: String) -> Result<Self, Error> {
        Ok(Self {
            name,
            version: parse_version(&version)?,
        })
    }
}
