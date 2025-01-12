use std::{io, path::Path, process::ExitStatus};

use crate::{
    config::Config,
    hash::HasIntegrity,
    lockfile::{LocalPackage, LocalPackageHashes, LockConstraint, PinnedState},
    lua_installation::LuaInstallation,
    operations::{self, FetchSrcRockError},
    package::PackageSpec,
    progress::{Progress, ProgressBar},
    remote_package_source::RemotePackageSource,
    rockspec::{Build as _, BuildBackendSpec, LuaVersionError, Rockspec},
    tree::{RockLayout, Tree},
};
pub(crate) mod utils;
use bon::{builder, Builder};
use cmake::CMakeError;
use command::CommandError;
use external_dependency::{ExternalDependencyError, ExternalDependencyInfo};

use indicatif::style::TemplateError;
use luarocks::LuarocksBuildError;
use make::MakeError;
use rust_mlua::RustError;
use ssri::Integrity;
use thiserror::Error;
use utils::recursive_copy_dir;

mod builtin;
mod cmake;
mod command;
mod luarocks;
mod make;
mod rust_mlua;

pub mod external_dependency;
pub mod variables;

/// A rocks package builder, providing fine-grained control
/// over how a package should be built.
#[derive(Builder)]
#[builder(start_fn = new, finish_fn(name = _build, vis = ""))]
pub struct Build<'a> {
    #[builder(start_fn)]
    rockspec: &'a Rockspec,
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
impl<State> BuildBuilder<'_, State>
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
    FetchSrcRockError(#[from] FetchSrcRockError),
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

async fn run_build(
    rockspec: &Rockspec,
    output_paths: &RockLayout,
    lua: &LuaInstallation,
    config: &Config,
    build_dir: &Path,
    progress: &Progress<ProgressBar>,
) -> Result<(), BuildError> {
    progress.map(|p| p.set_message("🛠️ Building..."));

    match rockspec.build.current_platform().build_backend.to_owned() {
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
            luarocks::build(rockspec, output_paths, lua, config, build_dir, progress).await?;
        }
        None => (),
    }

    Ok(())
}

async fn install(
    rockspec: &Rockspec,
    tree: &Tree,
    output_paths: &RockLayout,
    lua: &LuaInstallation,
    build_dir: &Path,
    progress: &Progress<ProgressBar>,
) -> Result<(), BuildError> {
    progress.map(|p| {
        p.set_message(format!(
            "💻 Installing {} {}",
            rockspec.package, rockspec.version
        ))
    });

    let install_spec = &rockspec.build.current_platform().install;
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

async fn do_build(build: Build<'_>) -> Result<LocalPackage, BuildError> {
    build.progress.map(|p| {
        p.set_message(format!(
            "Building {}@{}...",
            build.rockspec.package, build.rockspec.version
        ))
    });

    for (name, dep) in build.rockspec.external_dependencies.current_platform() {
        let _ = ExternalDependencyInfo::detect(name, dep, build.config.external_deps())?;
    }

    let lua_version = build.rockspec.lua_version_from_config(build.config)?;

    let tree = Tree::new(build.config.tree().clone(), lua_version.clone())?;

    let temp_dir = tempdir::TempDir::new(&build.rockspec.package.to_string())?;

    // Install the source in order to build.
    let rock_source = build.rockspec.source.current_platform();
    if let Err(err) = operations::FetchSrc::new(temp_dir.path(), rock_source, build.progress)
        .fetch_src()
        .await
    {
        let package = PackageSpec::new(
            build.rockspec.package.clone(),
            build.rockspec.version.clone(),
        );
        build.progress.map(|p| {
            p.println(format!(
                "⚠️ WARNING: Failed to fetch source for {}: {}",
                &package, err
            ))
        });
        build.progress.map(|p| {
            p.println(format!(
                "⚠️ Falling back to .src.rock archive from {}",
                &build.config.server()
            ))
        });
        operations::fetch_src_rock(&package, temp_dir.path(), build.config, build.progress).await?;
    }

    let hashes = LocalPackageHashes {
        rockspec: build.rockspec.hash()?,
        source: temp_dir.hash()?,
    };

    if let Some(expected) = &build.rockspec.source.current_platform().integrity {
        if expected.matches(&hashes.source).is_none() {
            return Err(BuildError::SourceIntegrityMismatch {
                expected: expected.clone(),
                actual: hashes.source,
            });
        }
    }

    let mut package = LocalPackage::from(
        &PackageSpec::new(
            build.rockspec.package.clone(),
            build.rockspec.version.clone(),
        ),
        build.constraint,
        build.source.unwrap_or_else(|| {
            RemotePackageSource::RockspecContent(build.rockspec.raw_content.clone())
        }),
        hashes,
    );
    package.spec.pinned = build.pin;

    match tree.lockfile()?.get(&package.id()) {
        Some(package) if build.behaviour == BuildBehaviour::NoForce => Ok(package.clone()),
        _ => {
            let output_paths = tree.rock(&package)?;

            let lua = LuaInstallation::new(&lua_version, build.config);

            let build_dir = match &rock_source.unpack_dir {
                Some(unpack_dir) => temp_dir.path().join(unpack_dir),
                None => temp_dir.path().into(),
            };

            run_build(
                build.rockspec,
                &output_paths,
                &lua,
                build.config,
                &build_dir,
                build.progress,
            )
            .await?;

            install(
                build.rockspec,
                &tree,
                &output_paths,
                &lua,
                &build_dir,
                build.progress,
            )
            .await?;

            for directory in &build.rockspec.build.current_platform().copy_directories {
                recursive_copy_dir(&build_dir.join(directory), &output_paths.etc)?;
            }

            Ok(package)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use predicates::prelude::*;
    use std::path::PathBuf;

    use assert_fs::{
        assert::PathAssert,
        prelude::{PathChild as _, PathCopy},
    };

    use crate::{
        config::{ConfigBuilder, LuaVersion},
        lua_installation::LuaInstallation,
        progress::MultiProgress,
        tree::RockLayout,
    };

    #[tokio::test]
    async fn test_builtin_build() {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/test/sample-project-no-build-spec");
        let build_dir = assert_fs::TempDir::new().unwrap();
        build_dir.copy_from(&project_root, &["**"]).unwrap();
        let dest_dir = assert_fs::TempDir::new().unwrap();
        let config = ConfigBuilder::new().build().unwrap();
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
            std::fs::read("resources/test/sample-project-no-build-spec/project.rockspec").unwrap(),
        )
        .unwrap();
        let rockspec = Rockspec::new(&rockspec_content).unwrap();
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
