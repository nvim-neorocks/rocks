use crate::lockfile::RemotePackageSourceUrl;
use crate::rockspec::LuaVersionCompatibility;
use crate::{lua_rockspec::LuaVersionError, rockspec::RemoteRockspec};
use std::{io, path::Path, process::ExitStatus};

use crate::{
    config::Config,
    hash::HasIntegrity,
    lockfile::{LocalPackage, LocalPackageHashes, LockConstraint, PinnedState},
    lua_installation::LuaInstallation,
    lua_rockspec::{Build as _, BuildBackendSpec, BuildInfo},
    operations::{self, FetchSrcError},
    package::PackageSpec,
    progress::{Progress, ProgressBar},
    remote_package_source::RemotePackageSource,
    tree::{RockLayout, Tree},
};
pub(crate) mod utils;
use bon::{builder, Builder};
use cmake::CMakeError;
use command::CommandError;
use external_dependency::{ExternalDependencyError, ExternalDependencyInfo};

use indicatif::style::TemplateError;
use itertools::Itertools;
use luarocks::LuarocksBuildError;
use make::MakeError;
use patch::{Patch, PatchError};
use rust_mlua::RustError;
use ssri::Integrity;
use thiserror::Error;
use utils::recursive_copy_dir;

mod builtin;
mod cmake;
mod command;
mod luarocks;
mod make;
mod patch;
mod rust_mlua;

pub mod external_dependency;
pub mod variables;

/// A rocks package builder, providing fine-grained control
/// over how a package should be built.
#[derive(Builder)]
#[builder(start_fn = new, finish_fn(name = _build, vis = ""))]
pub struct Build<'a, R: RemoteRockspec + HasIntegrity> {
    #[builder(start_fn)]
    rockspec: &'a R,
    #[builder(start_fn)]
    tree: &'a Tree,
    #[builder(start_fn)]
    config: &'a Config,
    #[builder(start_fn)]
    progress: &'a Progress<ProgressBar>,

    #[builder(default)]
    pin: PinnedState,
    #[builder(default)]
    constraint: LockConstraint,
    #[builder(default)]
    behaviour: BuildBehaviour,

    source: Option<RemotePackageSource>,
}

// Overwrite the `build()` function to use our own instead.
impl<R: RemoteRockspec + HasIntegrity, State> BuildBuilder<'_, R, State>
where
    State: build_builder::State + build_builder::IsComplete,
{
    pub async fn build(self) -> Result<LocalPackage, BuildError> {
        do_build(self._build()).await
    }
}

#[derive(Error, Debug)]
pub enum BuildError {
    #[error("IO operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("failed to create spinner: {0}")]
    SpinnerFailure(#[from] TemplateError),
    #[error(transparent)]
    ExternalDependencyError(#[from] ExternalDependencyError),
    #[error("source integrity mismatch.\nExpected: {expected},\nbut got: {actual}")]
    SourceIntegrityMismatch {
        expected: Integrity,
        actual: Integrity,
    },
    #[error(transparent)]
    PatchError(#[from] PatchError),
    #[error("failed to compile build modules: {0}")]
    CompilationError(#[from] cc::Error),
    #[error(transparent)]
    CMakeError(#[from] CMakeError),
    #[error(transparent)]
    MakeError(#[from] MakeError),
    #[error(transparent)]
    CommandError(#[from] CommandError),
    #[error(transparent)]
    RustError(#[from] RustError),
    #[error(transparent)]
    LuaVersionError(#[from] LuaVersionError),
    #[error("failed to fetch rock source: {0}")]
    FetchSrcError(#[from] FetchSrcError),
    #[error("compilation failed.\nstatus: {status}\nstdout: {stdout}\nstderr: {stderr}")]
    CommandFailure {
        status: ExitStatus,
        stdout: String,
        stderr: String,
    },
    #[error(transparent)]
    LuarocksBuildError(#[from] LuarocksBuildError),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BuildBehaviour {
    NoForce,
    Force,
}

impl Default for BuildBehaviour {
    fn default() -> Self {
        Self::NoForce
    }
}

impl From<bool> for BuildBehaviour {
    fn from(value: bool) -> Self {
        if value {
            Self::Force
        } else {
            Self::NoForce
        }
    }
}

async fn run_build<R: RemoteRockspec>(
    rockspec: &R,
    output_paths: &RockLayout,
    lua: &LuaInstallation,
    config: &Config,
    build_dir: &Path,
    progress: &Progress<ProgressBar>,
) -> Result<BuildInfo, BuildError> {
    progress.map(|p| p.set_message("🛠️ Building..."));

    Ok(
        match rockspec.build().current_platform().build_backend.to_owned() {
            Some(BuildBackendSpec::Builtin(build_spec)) => {
                build_spec
                    .run(output_paths, false, lua, config, build_dir, progress)
                    .await?
            }
            Some(BuildBackendSpec::Make(make_spec)) => {
                make_spec
                    .run(output_paths, false, lua, config, build_dir, progress)
                    .await?
            }
            Some(BuildBackendSpec::CMake(cmake_spec)) => {
                cmake_spec
                    .run(output_paths, false, lua, config, build_dir, progress)
                    .await?
            }
            Some(BuildBackendSpec::Command(command_spec)) => {
                command_spec
                    .run(output_paths, false, lua, config, build_dir, progress)
                    .await?
            }
            Some(BuildBackendSpec::RustMlua(rust_mlua_spec)) => {
                rust_mlua_spec
                    .run(output_paths, false, lua, config, build_dir, progress)
                    .await?
            }
            Some(BuildBackendSpec::LuaRock(_)) => {
                luarocks::build(rockspec, output_paths, lua, config, build_dir, progress).await?
            }
            None => BuildInfo::default(),
        },
    )
}

async fn install<R: RemoteRockspec>(
    rockspec: &R,
    tree: &Tree,
    output_paths: &RockLayout,
    lua: &LuaInstallation,
    build_dir: &Path,
    progress: &Progress<ProgressBar>,
) -> Result<(), BuildError> {
    progress.map(|p| {
        p.set_message(format!(
            "💻 Installing {} {}",
            rockspec.package(),
            rockspec.version()
        ))
    });

    let install_spec = &rockspec.build().current_platform().install;
    let lua_len = install_spec.lua.len();
    let lib_len = install_spec.lib.len();
    let bin_len = install_spec.bin.len();
    let total_len = lua_len + lib_len + bin_len;
    progress.map(|p| p.set_position(total_len as u64));

    if lua_len > 0 {
        progress.map(|p| p.set_message("Copying Lua modules..."));
    }
    for (target, source) in &install_spec.lua {
        let absolute_source = build_dir.join(source);
        utils::copy_lua_to_module_path(&absolute_source, target, &output_paths.src)?;
        progress.map(|p| p.set_position(p.position() + 1));
    }
    if lib_len > 0 {
        progress.map(|p| p.set_message("Compiling C libraries..."));
    }
    for (target, source) in &install_spec.lib {
        utils::compile_c_files(
            &vec![build_dir.join(source)],
            target,
            &output_paths.lib,
            lua,
        )?;
        progress.map(|p| p.set_position(p.position() + 1));
    }
    if lib_len > 0 {
        progress.map(|p| p.set_message("Copying binaries..."));
    }
    for (target, source) in &install_spec.bin {
        std::fs::copy(build_dir.join(source), tree.bin().join(target))?;
        progress.map(|p| p.set_position(p.position() + 1));
    }
    Ok(())
}

async fn do_build<R: RemoteRockspec + HasIntegrity>(
    build: Build<'_, R>,
) -> Result<LocalPackage, BuildError> {
    build.progress.map(|p| {
        p.set_message(format!(
            "Building {}@{}...",
            build.rockspec.package(),
            build.rockspec.version()
        ))
    });

    for (name, dep) in build.rockspec.external_dependencies().current_platform() {
        let _ = ExternalDependencyInfo::detect(name, dep, build.config.external_deps())?;
    }

    let lua_version = build.rockspec.lua_version_matches(build.config)?;

    let tree = build.tree;

    let temp_dir = tempdir::TempDir::new(&build.rockspec.package().to_string())?;

    let source_hash = operations::FetchSrc::new(
        temp_dir.path(),
        build.rockspec,
        build.config,
        build.progress,
    )
    .fetch()
    .await?;

    let hashes = LocalPackageHashes {
        rockspec: build.rockspec.hash()?,
        source: source_hash,
    };

    if let Some(expected) = &build.rockspec.source().current_platform().integrity {
        if expected.matches(&hashes.source).is_none() {
            return Err(BuildError::SourceIntegrityMismatch {
                expected: expected.clone(),
                actual: hashes.source,
            });
        }
    }

    let source_url = match &build.rockspec.source().current_platform().source_spec {
        crate::lua_rockspec::RockSourceSpec::Git(git_source) => RemotePackageSourceUrl::Git {
            url: format!("{}", &git_source.url),
        },
        crate::lua_rockspec::RockSourceSpec::File(path) => {
            RemotePackageSourceUrl::File { path: path.clone() }
        }
        crate::lua_rockspec::RockSourceSpec::Url(url) => {
            RemotePackageSourceUrl::Url { url: url.clone() }
        }
    };

    let mut package = LocalPackage::from(
        &PackageSpec::new(
            build.rockspec.package().clone(),
            build.rockspec.version().clone(),
        ),
        build.constraint,
        build.rockspec.binaries(),
        build.source.unwrap_or_else(|| {
            RemotePackageSource::RockspecContent(build.rockspec.to_rockspec_str())
        }),
        Some(source_url),
        hashes,
    );
    package.spec.pinned = build.pin;

    match tree.lockfile()?.get(&package.id()) {
        Some(package) if build.behaviour == BuildBehaviour::NoForce => Ok(package.clone()),
        _ => {
            let output_paths = tree.rock(&package)?;

            let lua = LuaInstallation::new(&lua_version, build.config);

            let rock_source = build.rockspec.source().current_platform();
            let build_dir = match &rock_source.unpack_dir {
                Some(unpack_dir) => temp_dir.path().join(unpack_dir),
                None => {
                    // Some older rockspecs don't specify a source.dir.
                    // If there exists a single directory after unpacking,
                    // we assume it's the source directory.
                    let entries = std::fs::read_dir(temp_dir.path())?
                        .filter_map(Result::ok)
                        .filter(|f| f.path().is_dir())
                        .collect_vec();
                    if entries.len() == 1 {
                        temp_dir.path().join(entries.first().unwrap().path())
                    } else {
                        temp_dir.path().into()
                    }
                }
            };

            Patch::new(
                &build_dir,
                &build.rockspec.build().current_platform().patches,
                build.progress,
            )
            .apply()?;

            let output = run_build(
                build.rockspec,
                &output_paths,
                &lua,
                build.config,
                &build_dir,
                build.progress,
            )
            .await?;

            package.spec.binaries.extend(output.binaries);

            install(
                build.rockspec,
                tree,
                &output_paths,
                &lua,
                &build_dir,
                build.progress,
            )
            .await?;

            for directory in build
                .rockspec
                .build()
                .current_platform()
                .copy_directories
                .iter()
                .filter(|dir| {
                    dir.file_name()
                        .is_some_and(|name| name != "doc" && name != "docs")
                })
            {
                recursive_copy_dir(&build_dir.join(directory), &output_paths.etc)?;
            }

            recursive_copy_doc_dir(&output_paths, &build_dir)?;

            std::fs::write(
                output_paths.rockspec_path(),
                build.rockspec.to_rockspec_str(),
            )?;

            Ok(package)
        }
    }
}

fn recursive_copy_doc_dir(output_paths: &RockLayout, build_dir: &Path) -> Result<(), BuildError> {
    let mut doc_dir = build_dir.join("doc");
    if !doc_dir.exists() {
        doc_dir = build_dir.join("docs");
    }
    recursive_copy_dir(&doc_dir, &output_paths.doc)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use predicates::prelude::*;
    use std::path::PathBuf;

    use assert_fs::{
        assert::PathAssert,
        prelude::{PathChild, PathCopy},
    };

    use crate::{
        config::{ConfigBuilder, LuaVersion},
        lua_installation::LuaInstallation,
        progress::MultiProgress,
        project::project_toml::PartialProjectToml,
        tree::RockLayout,
    };

    #[tokio::test]
    async fn test_builtin_build() {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/test/sample-project-no-build-spec");
        let build_dir = assert_fs::TempDir::new().unwrap();
        build_dir.copy_from(&project_root, &["**"]).unwrap();
        let dest_dir = assert_fs::TempDir::new().unwrap();
        let config = ConfigBuilder::new().unwrap().build().unwrap();
        let rock_layout = RockLayout {
            rock_path: dest_dir.to_path_buf(),
            etc: dest_dir.join("etc"),
            lib: dest_dir.join("lib"),
            src: dest_dir.join("src"),
            bin: dest_dir.join("bin"),
            conf: dest_dir.join("conf"),
            doc: dest_dir.join("doc"),
        };
        let lua = LuaInstallation::new(config.lua_version().unwrap_or(&LuaVersion::Lua51), &config);
        let rockspec_content = String::from_utf8(
            std::fs::read("resources/test/sample-project-no-build-spec/lux.toml").unwrap(),
        )
        .unwrap();
        let rockspec = PartialProjectToml::new(&rockspec_content)
            .unwrap()
            .into_remote()
            .unwrap();
        let progress = Progress::Progress(MultiProgress::new());
        run_build(
            &rockspec,
            &rock_layout,
            &lua,
            &config,
            &build_dir,
            &progress.map(|p| p.new_bar()),
        )
        .await
        .unwrap();
        let foo_dir = dest_dir.child("src").child("foo");
        foo_dir.assert(predicate::path::is_dir());
        let foo_init = foo_dir.child("init.lua");
        foo_init.assert(predicate::path::is_file());
        foo_init.assert(predicate::str::contains("return true"));
        let foo_bar_dir = foo_dir.child("bar");
        foo_bar_dir.assert(predicate::path::is_dir());
        let foo_bar_init = foo_bar_dir.child("init.lua");
        foo_bar_init.assert(predicate::path::is_file());
        foo_bar_init.assert(predicate::str::contains("return true"));
        let foo_bar_baz = foo_bar_dir.child("baz.lua");
        foo_bar_baz.assert(predicate::path::is_file());
        foo_bar_baz.assert(predicate::str::contains("return true"));
        let bin_file = dest_dir.child("bin").child("hello");
        bin_file.assert(predicate::path::is_file());
        bin_file.assert(predicate::str::contains("#!/usr/bin/env bash"));
        bin_file.assert(predicate::str::contains("echo \"Hello\""));
    }
}
