use pkg_config::{Config as PkgConfig, Library};
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::{
    config::external_deps::ExternalDependencySearchConfig, lua_rockspec::ExternalDependencySpec,
};

#[derive(Error, Debug)]
pub enum ExternalDependencyError {
    #[error("{}", not_found_error_msg(.0))]
    NotFound(String),
    #[error("IO error while trying to detect external dependencies: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug)]
pub enum ExternalDependencyInfo {
    /// Detected via pkg-config
    PkgConfig(Library),
    /// Library, detected via fallback mechanism
    Library {
        prefix: PathBuf,
        include_dir: PathBuf,
        lib_dir: PathBuf,
        lib_file: PathBuf,
    },
    /// Header-only dependency, detected vial fallback mechanism
    HeaderOnly {
        prefix: PathBuf,
        include_dir: PathBuf,
    },
}

impl ExternalDependencyInfo {
    pub fn detect(
        name: &str,
        dependency: &ExternalDependencySpec,
        config: &ExternalDependencySearchConfig,
    ) -> Result<Self, ExternalDependencyError> {
        let mut lib_info = PkgConfig::new()
            .print_system_libs(false)
            .cargo_metadata(false)
            .probe(&name.to_lowercase())
            .ok();
        if lib_info.is_none() {
            if let ExternalDependencySpec::Library(lib) = dependency {
                // Strip "lib" prefix and extension if present
                let file_name = lib.file_name().and_then(|f| f.to_str()).unwrap_or(name);
                let lib_name = if file_name.starts_with("lib") {
                    file_name.strip_prefix("lib").unwrap()
                } else {
                    file_name
                };
                let probe = if let Some(name_without_ext) = lib_name.split('.').next() {
                    name_without_ext
                } else {
                    lib_name
                };
                lib_info = PkgConfig::new()
                    .print_system_libs(false)
                    .cargo_metadata(false)
                    .probe(probe)
                    .ok();
            }
        }
        if let Some(info) = lib_info {
            match dependency {
                ExternalDependencySpec::Header(header) => {
                    if info
                        .include_paths
                        .iter()
                        .any(|path| path.join(header).exists())
                    {
                        return Ok(Self::PkgConfig(info));
                    }
                }
                ExternalDependencySpec::Library(_) => {
                    return Ok(Self::PkgConfig(info));
                }
            }
        }
        Self::fallback_detect(name, dependency, config)
    }

    fn fallback_detect(
        name: &str,
        dependency: &ExternalDependencySpec,
        config: &ExternalDependencySearchConfig,
    ) -> Result<Self, ExternalDependencyError> {
        let env_prefix = std::env::var(format!("{}_DIR", name.to_uppercase())).ok();

        let mut search_prefixes = Vec::new();
        if let Some(dir) = env_prefix {
            search_prefixes.push(PathBuf::from(dir));
        }
        if let Some(prefix) = config.prefixes.get(&format!("{}_DIR", name.to_uppercase())) {
            search_prefixes.push(prefix.clone());
        }
        search_prefixes.extend(config.search_prefixes.iter().cloned());

        match dependency {
            ExternalDependencySpec::Header(header) => {
                if let Some(inc_dir) = get_incdir(name, config) {
                    if inc_dir.join(header).exists() {
                        return Ok(Self::HeaderOnly {
                            prefix: inc_dir.parent().unwrap_or(&inc_dir).to_path_buf(),
                            include_dir: inc_dir,
                        });
                    }
                }

                // Search prefixes
                for prefix in search_prefixes {
                    let inc_dir = prefix.join(&config.include_subdir);
                    if inc_dir.join(header).exists() {
                        return Ok(Self::HeaderOnly {
                            prefix: prefix.clone(),
                            include_dir: inc_dir,
                        });
                    }
                }

                Err(ExternalDependencyError::NotFound(name.into()))
            }
            ExternalDependencySpec::Library(lib) => {
                // Check for specific directory overrides first
                if let (Some(inc_dir), Some(lib_dir)) =
                    (get_incdir(name, config), get_libdir(name, config))
                {
                    if library_exists(&lib_dir, lib, &config.lib_patterns) {
                        return Ok(Self::Library {
                            prefix: inc_dir.parent().unwrap_or(&inc_dir).to_path_buf(),
                            include_dir: inc_dir,
                            lib_dir,
                            lib_file: lib.clone(),
                        });
                    }
                }
                for prefix in search_prefixes {
                    let inc_dir = prefix.join(&config.include_subdir);
                    for lib_subdir in &config.lib_subdir {
                        let lib_dir = prefix.join(lib_subdir);
                        if library_exists(&lib_dir, lib, &config.lib_patterns) {
                            return Ok(Self::Library {
                                prefix: prefix.clone(),
                                include_dir: inc_dir,
                                lib_dir,
                                lib_file: lib.clone(),
                            });
                        }
                    }
                }

                Err(ExternalDependencyError::NotFound(name.into()))
            }
        }
    }
}

fn library_exists(lib_dir: &Path, lib: &Path, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        let file_name = pattern.replace('?', &format!("{}", lib.display()));
        lib_dir.join(&file_name).exists()
    })
}

fn get_incdir(name: &str, config: &ExternalDependencySearchConfig) -> Option<PathBuf> {
    let var_name = format!("{}_INCDIR", name.to_uppercase());
    if let Ok(env_incdir) = std::env::var(&var_name) {
        Some(env_incdir.into())
    } else {
        config.prefixes.get(&var_name).cloned()
    }
}

fn get_libdir(name: &str, config: &ExternalDependencySearchConfig) -> Option<PathBuf> {
    let var_name = format!("{}_LIBDIR", name.to_uppercase());
    if let Ok(env_incdir) = std::env::var(&var_name) {
        Some(env_incdir.into())
    } else {
        config.prefixes.get(&var_name).cloned()
    }
}

fn not_found_error_msg(name: &String) -> String {
    let env_dir = format!("{}_DIR", &name.to_uppercase());
    let env_inc = format!("{}_INCDIR", &name.to_uppercase());
    let env_lib = format!("{}_LIBDIR", &name.to_uppercase());

    format!(
        r#"External dependency not found: {}.
Consider one of the following:
1. Set environment variables:
   - {} for the installation prefix, or
   - {} and {} for specific directories
2. Add the installation prefix to the configuration:
   {} = "/path/to/installation""#,
        name, env_dir, env_inc, env_lib, env_dir,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::{prelude::*, TempDir};

    #[tokio::test]
    async fn test_detect_zlib_pkg_config_header() {
        // requires zlib to be in the nativeCheckInputs or dev environment
        let config = ExternalDependencySearchConfig::default();
        let result = ExternalDependencyInfo::detect(
            "zlib",
            &ExternalDependencySpec::Header("zlib.h".into()),
            &config,
        );
        assert!(matches!(result, Ok(ExternalDependencyInfo::PkgConfig(_))));
    }

    #[tokio::test]
    async fn test_detect_zlib_pkg_config_library_libz() {
        // requires zlib to be in the nativeCheckInputs or dev environment
        let config = ExternalDependencySearchConfig::default();
        let result = ExternalDependencyInfo::detect(
            "zlib",
            &ExternalDependencySpec::Library("libz".into()),
            &config,
        );
        assert!(matches!(result, Ok(ExternalDependencyInfo::PkgConfig(_))));
    }

    #[tokio::test]
    async fn test_detect_zlib_pkg_config_library_z() {
        // requires zlib to be in the nativeCheckInputs or dev environment
        let config = ExternalDependencySearchConfig::default();
        let result = ExternalDependencyInfo::detect(
            "zlib",
            &ExternalDependencySpec::Library("z".into()),
            &config,
        );
        assert!(matches!(result, Ok(ExternalDependencyInfo::PkgConfig(_))));
    }

    #[tokio::test]
    async fn test_detect_zlib_pkg_config_library_zlib() {
        // requires zlib to be in the nativeCheckInputs or dev environment
        let config = ExternalDependencySearchConfig::default();
        let result = ExternalDependencyInfo::detect(
            "zlib",
            &ExternalDependencySpec::Library("zlib".into()),
            &config,
        );
        assert!(matches!(result, Ok(ExternalDependencyInfo::PkgConfig(_))));
    }

    #[tokio::test]
    async fn test_fallback_detect_header_prefix() {
        let temp = TempDir::new().unwrap();
        let prefix_dir = temp.child("usr");
        let include_dir = prefix_dir.child("include");
        include_dir.create_dir_all().unwrap();

        let header = include_dir.child("foo.h");
        header.touch().unwrap();

        let mut config = ExternalDependencySearchConfig::default();
        config
            .prefixes
            .insert("FOO_DIR".into(), prefix_dir.path().to_path_buf());

        let result = ExternalDependencyInfo::fallback_detect(
            "foo",
            &ExternalDependencySpec::Header("foo.h".into()),
            &config,
        );

        assert!(matches!(
            result,
            Ok(ExternalDependencyInfo::HeaderOnly {
                include_dir: _,
                prefix: _,
            })
        ));
    }

    #[tokio::test]
    async fn test_fallback_detect_header_prefix_incdir() {
        let temp = TempDir::new().unwrap();
        let include_dir = temp.child("include");
        include_dir.create_dir_all().unwrap();

        let header = include_dir.child("foo.h");
        header.touch().unwrap();

        let mut config = ExternalDependencySearchConfig::default();
        config
            .prefixes
            .insert("FOO_INCDIR".into(), include_dir.path().to_path_buf());

        let result = ExternalDependencyInfo::fallback_detect(
            "foo",
            &ExternalDependencySpec::Header("foo.h".into()),
            &config,
        );

        assert!(matches!(
            result,
            Ok(ExternalDependencyInfo::HeaderOnly {
                include_dir: _,
                prefix: _,
            })
        ));
    }

    #[tokio::test]
    async fn test_fallback_detect_library_prefix() {
        let temp = TempDir::new().unwrap();
        let prefix_dir = temp.child("usr");
        let include_dir = prefix_dir.child("include");
        let lib_dir = prefix_dir.child("lib");
        include_dir.create_dir_all().unwrap();
        lib_dir.create_dir_all().unwrap();

        #[cfg(target_os = "linux")]
        let lib = lib_dir.child("libfoo.so");
        #[cfg(target_os = "macos")]
        let lib = lib_dir.child("libfoo.dylib");
        #[cfg(target_family = "windows")]
        let lib = lib_dir.child("foo.dll");

        lib.touch().unwrap();

        let mut config = ExternalDependencySearchConfig::default();
        config
            .prefixes
            .insert("FOO_DIR".to_string(), prefix_dir.path().to_path_buf());

        let result = ExternalDependencyInfo::fallback_detect(
            "foo",
            &ExternalDependencySpec::Library("foo".into()),
            &config,
        );

        assert!(matches!(
            result,
            Ok(ExternalDependencyInfo::Library {
                include_dir: _,
                lib_dir: _,
                prefix: _,
                lib_file: _,
            })
        ));
    }

    #[tokio::test]
    async fn test_fallback_detect_library_dirs() {
        let temp = TempDir::new().unwrap();

        let include_dir = temp.child("include");
        include_dir.create_dir_all().unwrap();

        let lib_dir = temp.child("lib");
        lib_dir.create_dir_all().unwrap();

        #[cfg(target_os = "linux")]
        let lib = lib_dir.child("libfoo.so");
        #[cfg(target_os = "macos")]
        let lib = lib_dir.child("libfoo.dylib");
        #[cfg(target_family = "windows")]
        let lib = lib_dir.child("foo.dll");

        lib.touch().unwrap();

        let mut config = ExternalDependencySearchConfig::default();
        config
            .prefixes
            .insert("FOO_INCDIR".into(), include_dir.path().to_path_buf());
        config
            .prefixes
            .insert("FOO_LIBDIR".into(), lib_dir.path().to_path_buf());

        let result = ExternalDependencyInfo::fallback_detect(
            "foo",
            &ExternalDependencySpec::Library("foo".into()),
            &config,
        );

        assert!(matches!(
            result,
            Ok(ExternalDependencyInfo::Library {
                include_dir: _,
                lib_dir: _,
                prefix: _,
                lib_file: _,
            })
        ));
    }

    #[tokio::test]
    async fn test_fallback_detect_search_prefixes() {
        let temp = TempDir::new().unwrap();
        let prefix_dir = temp.child("usr");
        let include_dir = prefix_dir.child("include");
        let lib_dir = prefix_dir.child("lib");
        include_dir.create_dir_all().unwrap();
        lib_dir.create_dir_all().unwrap();

        #[cfg(target_os = "linux")]
        let lib = lib_dir.child("libfoo.so");
        #[cfg(target_os = "macos")]
        let lib = lib_dir.child("libfoo.dylib");
        #[cfg(target_family = "windows")]
        let lib = lib_dir.child("foo.dll");

        lib.touch().unwrap();

        let mut config = ExternalDependencySearchConfig::default();
        config.search_prefixes.push(prefix_dir.path().to_path_buf());

        let result = ExternalDependencyInfo::fallback_detect(
            "foo",
            &ExternalDependencySpec::Library("foo".into()),
            &config,
        );

        assert!(matches!(
            result,
            Ok(ExternalDependencyInfo::Library {
                include_dir: _,
                lib_dir: _,
                prefix: _,
                lib_file: _,
            })
        ));
    }

    #[tokio::test]
    async fn test_fallback_detect_not_found() {
        let config = ExternalDependencySearchConfig::default();

        let result = ExternalDependencyInfo::fallback_detect(
            "foo",
            &ExternalDependencySpec::Header("foo.h".into()),
            &config,
        );

        assert!(matches!(result, Err(ExternalDependencyError::NotFound(_))));
    }
}
