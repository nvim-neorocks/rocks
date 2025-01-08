use std::env;
use std::io::Read;

use crate::project::rocks_toml::RocksTomlValidationError;
use crate::rockspec::Rockspec;
use crate::TOOL_VERSION;
use crate::{config::Config, project::Project};
use gpgme::{Context, Data};
use reqwest::{
    multipart::{Form, Part},
    Client,
};
use serde::Deserialize;
use serde_enum_str::Serialize_enum_str;
use thiserror::Error;
use url::Url;

/// A rocks package uploader, providing fine-grained control
/// over how a package should be uploaded.
pub struct ProjectUpload<'a> {
    project: Project,
    api_key: Option<ApiKey>,
    sign_protocol: SignatureProtocol,
    config: &'a Config,
}

impl<'a> ProjectUpload<'a> {
    /// Construct a new package uploader.
    pub fn new(project: Project, config: &'a Config) -> Self {
        Self {
            project,
            api_key: None,
            sign_protocol: SignatureProtocol::default(),
            config,
        }
    }

    /// Set the luarocks API key.
    pub fn api_key(self, api_key: ApiKey) -> Self {
        Self {
            api_key: Some(api_key),
            ..self
        }
    }

    /// Set the signature protocol.
    pub fn sign_protocol(self, sign_protocol: SignatureProtocol) -> Self {
        Self {
            sign_protocol,
            ..self
        }
    }

    /// Upload a package to a luarocks server.
    pub async fn upload_to_luarocks(self) -> Result<(), UploadError> {
        let api_key = self.api_key.unwrap_or(ApiKey::new()?);
        upload_from_project(&self.project, &api_key, self.sign_protocol, self.config).await
    }
}

#[derive(Deserialize, Debug)]
pub struct VersionCheckResponse {
    version: String,
}

#[derive(Error, Debug)]
pub enum ToolCheckError {
    #[error("error parsing tool check URL: {0}")]
    ParseError(#[from] url::ParseError),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error("`rocks` is out of date with {0}'s expected tool version! `rocks` is at version {TOOL_VERSION}, server is at {server_version}", server_version = _1.version)]
    ToolOutdated(String, VersionCheckResponse),
}

#[derive(Error, Debug)]
pub enum UserCheckError {
    #[error("error parsing user check URL: {0}")]
    ParseError(#[from] url::ParseError),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error("invalid API key provided")]
    UserNotFound,
}

#[derive(Error, Debug)]
#[error("could not check rock status on server: {0}")]
pub enum RockCheckError {
    #[error(transparent)]
    ParseError(#[from] url::ParseError),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

#[derive(Error, Debug)]
#[error(transparent)]
pub enum UploadError {
    #[error("error parsing upload URL: {0}")]
    ParseError(#[from] url::ParseError),
    Lua(#[from] mlua::Error),
    Request(#[from] reqwest::Error),
    RockCheck(#[from] RockCheckError),
    #[error("rock already exists on server: {0}")]
    RockExists(Url),
    #[error("unable to read rockspec: {0}")]
    RockspecRead(#[from] std::io::Error),
    #[error("{0}.\nHINT: If you'd like to skip the signing step supply `--sign-protocol none` to the CLI")]
    Signature(#[from] gpgme::Error),
    ToolCheck(#[from] ToolCheckError),
    UserCheck(#[from] UserCheckError),
    ApiKeyUnspecified(#[from] ApiKeyUnspecified),
    ValidationError(#[from] RocksTomlValidationError),
}

pub struct ApiKey(String);

#[derive(Error, Debug)]
#[error("no API key provided! Please set the $ROCKS_API_KEY variable")]
pub struct ApiKeyUnspecified;

impl ApiKey {
    /// Retrieves the rocks API key from the `$ROCKS_API_KEY` environment
    /// variable and seals it in this struct.
    pub fn new() -> Result<Self, ApiKeyUnspecified> {
        Ok(Self(
            env::var("ROCKS_API_KEY").map_err(|_| ApiKeyUnspecified)?,
        ))
    }

    /// Creates an API key from a String.
    ///
    /// # Safety
    ///
    /// This struct is designed to be sealed without a [`Display`](std::fmt::Display) implementation
    /// so that it can never accidentally be printed.
    ///
    /// Ensure that you do not do anything else with the API key string prior to sealing it in this
    /// struct.
    pub unsafe fn from(str: String) -> Self {
        Self(str)
    }

    /// Retrieves the underlying API key as a [`String`].
    ///
    /// # Safety
    ///
    /// Strings may accidentally be printed as part of its [`Display`](std::fmt::Display)
    /// implementation. Ensure that you never pass this variable somewhere it may be displayed.
    pub unsafe fn get(&self) -> &String {
        &self.0
    }
}

#[derive(Serialize_enum_str, Default, Clone)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "clap", clap(rename_all = "lowercase"))]
#[serde(rename_all = "lowercase")]
pub enum SignatureProtocol {
    Assuan,
    CMS,
    #[default]
    Default,
    G13,
    GPGConf,
    None,
    OpenPGP,
    Spawn,
    UIServer,
}

impl From<SignatureProtocol> for gpgme::Protocol {
    fn from(val: SignatureProtocol) -> Self {
        match val {
            SignatureProtocol::Default => gpgme::Protocol::Default,
            SignatureProtocol::OpenPGP => gpgme::Protocol::OpenPgp,
            SignatureProtocol::CMS => gpgme::Protocol::Cms,
            SignatureProtocol::GPGConf => gpgme::Protocol::GpgConf,
            SignatureProtocol::Assuan => gpgme::Protocol::Assuan,
            SignatureProtocol::G13 => gpgme::Protocol::G13,
            SignatureProtocol::UIServer => gpgme::Protocol::UiServer,
            SignatureProtocol::Spawn => gpgme::Protocol::Spawn,
            SignatureProtocol::None => unreachable!(),
        }
    }
}

async fn upload_from_project(
    project: &Project,
    api_key: &ApiKey,
    protocol: SignatureProtocol,
    config: &Config,
) -> Result<(), UploadError> {
    let client = Client::builder().https_only(true).build()?;

    helpers::ensure_tool_version(&client, config.server()).await?;
    helpers::ensure_user_exists(&client, api_key, config.server()).await?;

    let rocks = project.rocks().into_validated_rocks_toml()?;

    if helpers::rock_exists(
        &client,
        api_key,
        rocks.package(),
        rocks.version(),
        config.server(),
    )
    .await?
    {
        return Err(UploadError::RockExists(config.server().clone()));
    }

    let rockspec_content = std::fs::read_to_string(project.root().join("project.rockspec"))?;

    let signed = if let SignatureProtocol::None = protocol {
        None
    } else {
        let mut ctx = Context::from_protocol(protocol.into())?;
        let mut signature = Data::new()?;

        ctx.set_armor(true);
        ctx.sign_detached(rockspec_content.clone(), &mut signature)?;

        let mut signature_str = String::new();
        signature.read_to_string(&mut signature_str)?;

        Some(signature_str)
    };

    let rockspec = Part::text(rockspec_content)
        .file_name(format!("{}-{}.rockspec", rocks.package(), rocks.version()))
        .mime_str("application/octet-stream")?;

    let multipart = {
        let multipart = Form::new().part("rockspec_file", rockspec);

        match signed {
            Some(signature) => {
                let part = Part::text(signature).file_name("project.rockspec.sig");
                multipart.part("rockspec_sig", part)
            }
            None => multipart,
        }
    };

    client
        .post(helpers::url_for_method(config.server(), api_key, "upload")?)
        .multipart(multipart)
        .send()
        .await?;

    Ok(())
}

mod helpers {
    use super::*;
    use crate::package::{PackageName, PackageVersion};
    use crate::upload::RockCheckError;
    use crate::upload::{ToolCheckError, UserCheckError};
    use reqwest::Client;
    use url::Url;

    pub(crate) fn url_for_method(
        server_url: &Url,
        api_key: &ApiKey,
        endpoint: &str,
    ) -> Result<Url, url::ParseError> {
        let api_key = unsafe { api_key.get() };
        server_url
            .join("api/1")
            .expect("error constructing 'api/1' path")
            .join(api_key)?
            .join(endpoint)
    }

    pub(crate) async fn ensure_tool_version(
        client: &Client,
        server_url: &Url,
    ) -> Result<(), ToolCheckError> {
        let url = server_url.join("api/tool_version")?;
        let response: VersionCheckResponse = client
            .post(url)
            .json(&("current", TOOL_VERSION))
            .send()
            .await?
            .json()
            .await?;

        if response.version == TOOL_VERSION {
            Ok(())
        } else {
            Err(ToolCheckError::ToolOutdated(
                server_url.to_string(),
                response,
            ))
        }
    }

    pub(crate) async fn ensure_user_exists(
        client: &Client,
        api_key: &ApiKey,
        server_url: &Url,
    ) -> Result<(), UserCheckError> {
        client
            .get(url_for_method(server_url, api_key, "status")?)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }

    pub(crate) async fn rock_exists(
        client: &Client,
        api_key: &ApiKey,
        name: &PackageName,
        version: &PackageVersion,
        server: &Url,
    ) -> Result<bool, RockCheckError> {
        Ok(client
            .get(url_for_method(server, api_key, "check_rockspec")?)
            .query(&(
                ("package", name.to_string()),
                ("version", version.to_string()),
            ))
            .send()
            .await?
            .text()
            .await?
            != "{}")
    }
}
