use crate::config::{Config, LuaVersionUnset};
use crate::lua_installation::LuaInstallation;
use crate::lua_rockspec::Build;
use crate::lua_rockspec::{BuildInfo, TreesitterParserBuildSpec};
use crate::progress::{Progress, ProgressBar};
use crate::tree::RockLayout;
use std::io;
use std::num::ParseIntError;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tree_sitter_generate::GenerateError;

const DEFAULT_GENERATE_ABI_VERSION: usize = tree_sitter::LANGUAGE_VERSION;

#[derive(Error, Debug)]
pub enum TreesitterBuildError {
    #[error(transparent)]
    LuaVersionUnset(#[from] LuaVersionUnset),
    #[error("failed to initialise the tree-sitter loader: {0}")]
    Loader(String),
    #[error("invalid TREE_SITTER_LANGUAGE_VERSION: {0}")]
    ParseAbiVersion(#[from] ParseIntError),
    #[error("error generating tree-sitter grammar: {0}")]
    Generate(#[from] GenerateError),
    #[error("error compiling the tree-sitter grammar: {0}")]
    TreesitterCompileError(String),
    #[error("error creating directory {dir}: {err}")]
    CreateDir { dir: PathBuf, err: io::Error },
    #[error("error writing query file: {0}")]
    WriteQuery(io::Error),
}

impl Build for TreesitterParserBuildSpec {
    type Err = TreesitterBuildError;

    async fn run(
        self,
        output_paths: &RockLayout,
        _no_install: bool,
        _lua: &LuaInstallation,
        _config: &Config,
        build_dir: &Path,
        progress: &Progress<ProgressBar>,
    ) -> Result<BuildInfo, Self::Err> {
        let build_dir = self
            .location
            .map(|dir| build_dir.join(dir))
            .unwrap_or(build_dir.to_path_buf());
        if self.generate {
            progress.map(|b| b.set_message("📖 ✍Generating tree-sitter grammar..."));
            let abi_version = match std::env::var("TREE_SITTER_LANGUAGE_VERSION") {
                Ok(v) => v.parse()?,
                Err(_) => DEFAULT_GENERATE_ABI_VERSION,
            };
            tree_sitter_generate::generate_parser_in_directory(
                &build_dir,
                None,
                None,
                abi_version,
                None,
                None,
            )?;
        }
        progress.map(|b| b.set_message("🌳 Building tree-sitter parser..."));
        if self.parser {
            let parser_dir = output_paths.etc.join("parser");
            tokio::fs::create_dir_all(&parser_dir)
                .await
                .map_err(|err| TreesitterBuildError::CreateDir {
                    dir: parser_dir.clone(),
                    err,
                })?;
            let mut loader = tree_sitter_loader::Loader::new()
                .map_err(|err| TreesitterBuildError::Loader(err.to_string()))?;
            let output_path =
                parser_dir.join(format!("{}.{}", self.lang, std::env::consts::DLL_EXTENSION));
            loader.force_rebuild(true);
            loader
                .compile_parser_at_path(&build_dir, output_path, &[])
                .map_err(|err| TreesitterBuildError::TreesitterCompileError(err.to_string()))?;
        }

        let queries_dir = output_paths.etc.join("queries");
        if !self.queries.is_empty() {
            tokio::fs::create_dir_all(&queries_dir)
                .await
                .map_err(|err| TreesitterBuildError::CreateDir {
                    dir: queries_dir.clone(),
                    err,
                })?;
        }
        for (path, content) in self.queries {
            let dest = queries_dir.join(path);
            tokio::fs::write(&dest, content)
                .await
                .map_err(TreesitterBuildError::WriteQuery)?;
        }

        Ok(BuildInfo::default())
    }
}
