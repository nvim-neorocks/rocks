use eyre::{eyre, Result};
use git_url_parse::GitUrl;
use mlua::{FromLua, Lua, Value};
use regex::RegexSet;
use reqwest::Url;
use serde::{de, Deserialize, Deserializer};
use ssri::Integrity;
use std::{borrow::Cow, path::PathBuf, str::FromStr};

use super::{
    FromPlatformOverridable, PartialOverride, PerPlatform, PerPlatformWrapper, PlatformOverridable,
};

#[derive(Debug, PartialEq)]
pub struct RockSource {
    pub source_spec: RockSourceSpec,
    pub integrity: Option<Integrity>,
    pub archive_name: String,
    pub unpack_dir: String,
}

impl FromPlatformOverridable<RockSourceInternal, Self> for RockSource {
    fn from_platform_overridable(internal: RockSourceInternal) -> Result<Self> {
        // The rockspec.source table allows invalid combinations
        // This ensures that invalid combinations are caught while parsing.
        let url = internal.url.ok_or(eyre!("source URL missing"))?;
        let archive_name = internal.file.unwrap_or(url.derive_file_name()?);

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

        let dir = internal.dir.unwrap_or(
            PathBuf::from(&archive_name)
                .file_stem()
                .and_then(|name| name.to_str())
                .map(str::to_string)
                .ok_or(eyre!("could not derive rockspec source.dir"))?,
        );
        Ok(RockSource {
            source_spec,
            integrity: internal.hash,
            archive_name,
            unpack_dir: dir,
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
    dir: Option<String>,
    tag: Option<String>,
    branch: Option<String>,
    module: Option<String>,
}

impl PartialOverride for RockSourceInternal {
    fn apply_overrides(&self, override_spec: &Self) -> Self {
        Self {
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
        }
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

impl SourceUrl {
    fn derive_file_name(&self) -> Result<String> {
        let ret = match self {
            SourceUrl::Cvs(str) => base_name(str.as_str()).to_string(),
            SourceUrl::File(file) => file
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .ok_or(eyre!("could not derive rockspec source.file"))?,
            SourceUrl::Url(url) => base_name(url.to_string().as_str()).to_string(),
            SourceUrl::Git(url) => base_name(url.to_string().as_str()).to_string(),
            SourceUrl::Mercurial(url) => base_name(url.to_string().as_str()).to_string(),
            SourceUrl::Sscm(url) => base_name(url.to_string().as_str()).to_string(),
            SourceUrl::Svn(url) => base_name(url.to_string().as_str()).to_string(),
        };
        Ok(ret)
    }
}

impl FromStr for SourceUrl {
    type Err = eyre::Error;

    fn from_str(str: &str) -> Result<Self> {
        let url_regex_set: RegexSet =
            RegexSet::new([r"^https://", r"^http://", r"^ftp://"]).unwrap();

        let mercurial_source_regex_set: RegexSet =
            RegexSet::new([r"^hg://", r"^hg\+http://", r"^hg\+https://", r"^hg\+ssh://"]).unwrap();

        match str {
            s if s.starts_with("cvs://") => Ok(Self::Cvs(s.to_string())),
            s if s.starts_with("file://") => Ok(Self::File(s.trim_start_matches("file://").into())),
            s if s.starts_with("git://") => Ok(Self::Git(s.parse()?)),
            s if s.starts_with("git+") => Ok(Self::Git(s.trim_start_matches("git+").parse()?)),
            s if url_regex_set.matches(s).matched_any() => Ok(Self::Url(s.parse()?)),
            s if mercurial_source_regex_set.matches(s).matched_any() => {
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

#[cfg(test)]
mod tests {

    use std::path::Path;

    use super::*;

    #[tokio::test]
    async fn parse_source_url() {
        let url: SourceUrl = "cvs://foo".parse().unwrap();
        assert_eq!(url, SourceUrl::Cvs("cvs://foo".into()));
        let url: SourceUrl = "cvs://bar".parse().unwrap();
        assert_eq!(url, SourceUrl::Cvs("cvs://bar".into()));
        let url: SourceUrl = "file:///tmp/foo".parse().unwrap();
        let file = Path::new("/tmp/foo");
        assert_eq!(url, SourceUrl::File(file.to_path_buf()));
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
