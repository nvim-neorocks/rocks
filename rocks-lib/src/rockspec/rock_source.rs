use eyre::{eyre, Result};
use git_url_parse::GitUrl;
use mlua::{FromLua, Lua, Value};
use reqwest::Url;
use serde::{de, Deserialize, Deserializer};
use ssri::Integrity;
use std::{borrow::Cow, fs, path::PathBuf, str::FromStr};

use super::{
    FromPlatformOverridable, PartialOverride, PerPlatform, PerPlatformWrapper, PlatformOverridable,
};

#[derive(Debug, PartialEq)]
pub struct RockSource {
    pub source_spec: RockSourceSpec,
    pub integrity: Option<Integrity>,
    pub archive_name: Option<String>,
    pub unpack_dir: Option<PathBuf>,
}

impl FromPlatformOverridable<RockSourceInternal, Self> for RockSource {
    fn from_platform_overridable(internal: RockSourceInternal) -> Result<Self> {
        // The rockspec.source table allows invalid combinations
        // This ensures that invalid combinations are caught while parsing.
        let url = internal.url.ok_or(eyre!("source URL missing"))?;

        let source_spec = match (url, internal.tag, internal.branch, internal.module) {
            (source, None, None, None) => Ok(RockSourceSpec::default_from_source_url(source)),
            (SourceUrl::Cvs(url), None, None, Some(module)) => {
                Ok(RockSourceSpec::Cvs(CvsSource { url, module }))
            }
            (SourceUrl::Git(url), Some(tag), None, None) => Ok(RockSourceSpec::Git(GitSource {
                url,
                checkout_ref: Some(tag),
            })),
            (SourceUrl::Git(url), None, Some(branch), None) => Ok(RockSourceSpec::Git(GitSource {
                url,
                checkout_ref: Some(branch),
            })),
            (SourceUrl::Mercurial(url), Some(tag), None, None) => {
                Ok(RockSourceSpec::Mercurial(MercurialSource {
                    url,
                    checkout_ref: Some(tag),
                }))
            }
            (SourceUrl::Mercurial(url), None, Some(branch), None) => {
                Ok(RockSourceSpec::Mercurial(MercurialSource {
                    url,
                    checkout_ref: Some(branch),
                }))
            }
            (SourceUrl::Sscm(url), None, None, Some(module)) => {
                Ok(RockSourceSpec::Sscm(SscmSource { url, module }))
            }
            (SourceUrl::Svn(url), tag, None, module) => {
                Ok(RockSourceSpec::Svn(SvnSource { url, tag, module }))
            }
            _ => Err(eyre!("invalid rockspec source field combination.")),
        }?;

        Ok(RockSource {
            source_spec,
            integrity: internal.hash,
            archive_name: internal.file,
            unpack_dir: internal.dir,
        })
    }
}

impl<'lua> FromLua<'lua> for PerPlatform<RockSource> {
    fn from_lua(value: Value<'lua>, lua: &'lua Lua) -> mlua::Result<Self> {
        let wrapper = PerPlatformWrapper::from_lua(value, lua)?;
        Ok(wrapper.un_per_platform)
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum RockSourceSpec {
    Cvs(CvsSource),
    Git(GitSource),
    File(PathBuf),
    Url(Url),
    Mercurial(MercurialSource),
    Sscm(SscmSource),
    Svn(SvnSource),
}

impl RockSourceSpec {
    fn default_from_source_url(url: SourceUrl) -> Self {
        match url {
            SourceUrl::Cvs(url) => Self::Cvs(CvsSource {
                module: base_name(url.as_str()).into(),
                url,
            }),
            SourceUrl::File(path) => Self::File(path),
            SourceUrl::Url(url) => Self::Url(url),
            SourceUrl::Git(url) => Self::Git(GitSource {
                url,
                checkout_ref: None,
            }),
            SourceUrl::Mercurial(url) => Self::Mercurial(MercurialSource {
                url,
                checkout_ref: None,
            }),
            SourceUrl::Sscm(url) => Self::Sscm(SscmSource {
                module: base_name(url.as_str()).into(),
                url,
            }),
            SourceUrl::Svn(url) => Self::Svn(SvnSource {
                url,
                module: None,
                tag: None,
            }),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct CvsSource {
    pub url: String,
    pub module: String,
}

#[derive(Debug, PartialEq, Clone)]
pub struct GitSource {
    pub url: GitUrl,
    pub checkout_ref: Option<String>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct MercurialSource {
    pub url: String,
    pub checkout_ref: Option<String>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct SscmSource {
    pub url: String,
    pub module: String,
}

#[derive(Debug, PartialEq, Clone)]
pub struct SvnSource {
    pub url: String,
    pub module: Option<String>,
    pub tag: Option<String>,
}

/// Used as a helper for Deserialize,
/// because the Rockspec schema allows invalid rockspecs (╯°□°)╯︵ ┻━┻
#[derive(Debug, PartialEq, Deserialize, Clone, Default)]
struct RockSourceInternal {
    #[serde(default, deserialize_with = "source_url_from_str")]
    url: Option<SourceUrl>,
    #[serde(deserialize_with = "integrity_opt_from_hash_str")]
    #[serde(default)]
    hash: Option<Integrity>,
    file: Option<String>,
    dir: Option<PathBuf>,
    tag: Option<String>,
    branch: Option<String>,
    module: Option<String>,
}

impl PartialOverride for RockSourceInternal {
    fn apply_overrides(&self, override_spec: &Self) -> Result<Self> {
        Ok(Self {
            url: override_opt(override_spec.url.as_ref(), self.url.as_ref()),
            hash: override_opt(override_spec.hash.as_ref(), self.hash.as_ref()),
            file: override_opt(override_spec.file.as_ref(), self.file.as_ref()),
            dir: override_opt(override_spec.dir.as_ref(), self.dir.as_ref()),
            tag: match (&override_spec.branch, &override_spec.module) {
                (None, None) => override_opt(override_spec.tag.as_ref(), self.tag.as_ref()),
                _ => None,
            },
            branch: match (&override_spec.tag, &override_spec.module) {
                (None, None) => override_opt(override_spec.branch.as_ref(), self.branch.as_ref()),
                _ => None,
            },
            module: match (&override_spec.tag, &override_spec.branch) {
                (None, None) => override_opt(override_spec.module.as_ref(), self.module.as_ref()),
                _ => None,
            },
        })
    }
}

impl PlatformOverridable for RockSourceInternal {
    fn on_nil<T>() -> Result<PerPlatform<T>>
    where
        T: PlatformOverridable,
    {
        Err(eyre!("Missing source."))
    }
}

fn override_opt<T: Clone>(override_opt: Option<&T>, base: Option<&T>) -> Option<T> {
    override_opt.or(base).cloned()
}

/// Internal helper for parsing
#[derive(Debug, PartialEq, Clone)]
enum SourceUrl {
    /// For the CVS source control manager
    Cvs(String),
    /// For URLs in the local filesystem
    File(PathBuf),
    /// Web URLs
    Url(Url),
    /// For the Git source control manager
    Git(GitUrl),
    /// or the Mercurial source control manager
    Mercurial(String),
    /// or the SurroundSCM source control manager
    Sscm(String),
    /// or the Subversion source control manager
    Svn(String),
}

impl FromStr for SourceUrl {
    type Err = eyre::Error;

    fn from_str(str: &str) -> Result<Self> {
        match str {
            s if s.starts_with("cvs://") => Ok(Self::Cvs(s.to_string())),
            s if s.starts_with("file://") => {
                let path_buf: PathBuf = s.trim_start_matches("file://").into();
                let path = fs::canonicalize(&path_buf)?;
                Ok(Self::File(path))
            }
            s if s.starts_with("git://") => Ok(Self::Git(s.parse()?)),
            s if starts_with_any(
                s,
                ["git+file://", "git+http://", "git+https://", "git+ssh://"].into(),
            ) =>
            {
                Ok(Self::Git(s.trim_start_matches("git+").parse()?))
            }
            s if starts_with_any(s, ["https://", "http://", "ftp://"].into()) => {
                Ok(Self::Url(s.parse()?))
            }
            s if starts_with_any(
                s,
                ["hg://", "hg+http://", "hg+https://", "hg+ssh://"].into(),
            ) =>
            {
                Ok(Self::Mercurial(s.to_string()))
            }
            s if s.starts_with("sscm://") => Ok(Self::Sscm(s.to_string())),
            s if s.starts_with("svn://") => Ok(Self::Svn(s.to_string())),
            s => Err(eyre!("Unsupported source URL: {}", s)),
        }
    }
}

fn source_url_from_str<'de, D>(deserializer: D) -> Result<Option<SourceUrl>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    let source_url = match opt {
        Some(s) => Some(SourceUrl::from_str(s.as_str()).map_err(de::Error::custom)?),
        None => None,
    };
    Ok(source_url)
}

fn integrity_opt_from_hash_str<'de, D>(deserializer: D) -> Result<Option<Integrity>, D::Error>
where
    D: Deserializer<'de>,
{
    let str_opt: Option<String> = Option::deserialize(deserializer)?;
    let integrity_opt = match str_opt {
        Some(s) => Some(s.parse().map_err(de::Error::custom)?),
        None => None,
    };
    Ok(integrity_opt)
}

/// Implementation of the luarocks base_name function
/// Strips the path, so /a/b/c becomes c
fn base_name(path: &str) -> Cow<'_, str> {
    let mut pieces = path.rsplit('/');
    match pieces.next() {
        Some(p) => p.into(),
        None => path.into(),
    }
}

fn starts_with_any(str: &str, prefixes: Vec<&str>) -> bool {
    return prefixes.iter().any(|&prefix| str.starts_with(prefix));
}

#[cfg(test)]
mod tests {

    use tempdir::TempDir;

    use super::*;

    #[tokio::test]
    async fn parse_source_url() {
        let url: SourceUrl = "cvs://foo".parse().unwrap();
        assert_eq!(url, SourceUrl::Cvs("cvs://foo".into()));
        let url: SourceUrl = "cvs://bar".parse().unwrap();
        assert_eq!(url, SourceUrl::Cvs("cvs://bar".into()));
        let dir = TempDir::new("rocks-test").unwrap().into_path();
        let url: SourceUrl = format!("file://{}", dir.to_string_lossy()).parse().unwrap();
        assert_eq!(url, SourceUrl::File(dir));
        let url: SourceUrl = "ftp://example.com/foo".parse().unwrap();
        assert!(matches!(url, SourceUrl::Url { .. }));
        let url: SourceUrl = "git://example.com/foo".parse().unwrap();
        assert!(matches!(url, SourceUrl::Git { .. }));
        let url: SourceUrl = "git+file://example.com/foo".parse().unwrap();
        assert!(matches!(url, SourceUrl::Git { .. }));
        let url: SourceUrl = "git+http://example.com/foo".parse().unwrap();
        assert!(matches!(url, SourceUrl::Git { .. }));
        let url: SourceUrl = "git+https://example.com/foo".parse().unwrap();
        assert!(matches!(url, SourceUrl::Git { .. }));
        let url: SourceUrl = "git+ssh://example.com/foo".parse().unwrap();
        assert!(matches!(url, SourceUrl::Git { .. }));
        let _err = SourceUrl::from_str("git+foo://example.com/foo").unwrap_err();
        let url: SourceUrl = "hg://example.com/foo".parse().unwrap();
        assert!(matches!(url, SourceUrl::Mercurial { .. }));
        let url: SourceUrl = "hg+http://example.com/foo".parse().unwrap();
        assert!(matches!(url, SourceUrl::Mercurial { .. }));
        let url: SourceUrl = "hg+https://example.com/foo".parse().unwrap();
        assert!(matches!(url, SourceUrl::Mercurial { .. }));
        let url: SourceUrl = "hg+ssh://example.com/foo".parse().unwrap();
        assert!(matches!(url, SourceUrl::Mercurial { .. }));
        let _err = SourceUrl::from_str("hg+foo://example.com/foo").unwrap_err();
        let url: SourceUrl = "https://example.com/foo".parse().unwrap();
        assert!(matches!(url, SourceUrl::Url { .. }));
        let url: SourceUrl = "http://example.com/foo".parse().unwrap();
        assert!(matches!(url, SourceUrl::Url { .. }));
        let url: SourceUrl = "sscm://foo".parse().unwrap();
        assert_eq!(url, SourceUrl::Sscm("sscm://foo".into()));
        let url: SourceUrl = "sscm://bar".parse().unwrap();
        assert_eq!(url, SourceUrl::Sscm("sscm://bar".into()));
        let url: SourceUrl = "svn://foo".parse().unwrap();
        assert_eq!(url, SourceUrl::Svn("svn://foo".into()));
        let url: SourceUrl = "svn://bar".parse().unwrap();
        assert_eq!(url, SourceUrl::Svn("svn://bar".into()));
    }
}
