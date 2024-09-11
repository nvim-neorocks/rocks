use eyre::Result;
use html_escape::decode_html_entities;
use semver::{Error, Version, VersionReq};

/// Parses a Version from a string, automatically supplying any missing details (i.e. missing
/// minor/patch sections).
pub fn parse_version(s: &str) -> Result<Version, Error> {
    Version::parse(&append_minor_patch_if_missing(s.to_string()))
}

/// Transform LuaRocks constraints into constraints that can be parsed by the semver crate.
pub fn parse_version_req(version_constraints: &str) -> Result<VersionReq, Error> {
    let unescaped = decode_html_entities(version_constraints)
        .to_string()
        .as_str()
        .to_owned();
    let transformed = match unescaped {
        s if s.starts_with("~>") => parse_pessimistic_version_constraint(s)?,
        // The semver crate only understands "= version", unlike luarocks which understands "== version".
        s if s.starts_with("==") => s[1..].to_string(),
        s => s,
    };

    let version_req = VersionReq::parse(&transformed)?;
    Ok(version_req)
}

fn parse_pessimistic_version_constraint(version_constraint: String) -> Result<String, Error> {
    // pessimistic operator
    let min_version_str = &version_constraint[2..].trim();
    let min_version = Version::parse(&append_minor_patch_if_missing(min_version_str.to_string()))?;

    let max_version = match min_version_str.matches('.').count() {
        0 => Version {
            major: &min_version.major + 1,
            ..min_version.clone()
        },
        1 => Version {
            minor: &min_version.minor + 1,
            ..min_version.clone()
        },
        _ => Version {
            patch: &min_version.patch + 1,
            ..min_version.clone()
        },
    };

    Ok(format!(">= {min_version}, < {max_version}"))
}

/// Recursively append .0 until the version string has a minor or patch version
fn append_minor_patch_if_missing(version: String) -> String {
    if version.matches('.').count() < 2 {
        append_minor_patch_if_missing(version + ".0")
    } else {
        version
    }
}
