use pkg_config::{Config as PkgConfig, Library};
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::{
    config::external_deps::ExternalDependencySearchConfig, rockspec::ExternalDependencySpec,
};

#[derive(Error, Debug)]
pub enum ExternalDependencyError {
    #[error("{}", not_found_error_msg(.0))]
    NotFound(String),
    #[error("IO error while trying to detect external dependencies: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug)]
pub(crate) enum ExternalDependencyInfo {
    /// Detected via pkg-config
    PkgConfig(Library),
    /// Library, detected via fallback mechanism
    Library {
        prefix: PathBuf,
        include_dir: PathBuf,
        lib_dir: PathBuf,
        lib_name: String,
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
        let probe = match dependency {
            ExternalDependencySpec::Header(_) => &name.to_lowercase(),
            ExternalDependencySpec::Library(lib) => lib,
        };
        let lib_info = PkgConfig::new()
            .print_system_libs(false)
            .cargo_metadata(false)
            .probe(probe)
            .ok();
        if let Some(info) = lib_info {
            match dependency {
                ExternalDependencySpec::Header(header) => {
                    if info
                        .include_paths
                        .iter()
                        .any(|path| path.join(header).exists())
                    {
                        return Ok(ExternalDependencyInfo::PkgConfig(info));
                    }
                }
                ExternalDependencySpec::Library(_) => {
                    return Ok(ExternalDependencyInfo::PkgConfig(info));
                }
            }
        }

        // Fallback

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
                        return Ok(ExternalDependencyInfo::HeaderOnly {
                            prefix: inc_dir.parent().unwrap_or(&inc_dir).to_path_buf(),
                            include_dir: inc_dir,
                        });
                    }
                }

                // Search prefixes
                for prefix in search_prefixes {
                    let inc_dir = prefix.join(&config.include_subdir);
                    if inc_dir.join(header).exists() {
                        return Ok(ExternalDependencyInfo::HeaderOnly {
                            prefix: prefix.clone(),
                            include_dir: inc_dir,
                        });
                    }
                }

                Err(ExternalDependencyError::NotFound(header.clone()))
            }
            ExternalDependencySpec::Library(lib) => {
                // Check for specific directory overrides first
                if let (Some(inc_dir), Some(lib_dir)) =
                    (get_incdir(name, config), get_libdir(name, config))
                {
                    if library_exists(&lib_dir, lib, &config.lib_patterns) {
                        return Ok(ExternalDependencyInfo::Library {
                            prefix: inc_dir.parent().unwrap_or(&inc_dir).to_path_buf(),
                            include_dir: inc_dir,
                            lib_dir,
                            lib_name: lib.clone(),
                        });
                    }
                }
                for prefix in search_prefixes {
                    let inc_dir = prefix.join(&config.include_subdir);
                    for lib_subdir in &config.lib_subdir {
                        let lib_dir = prefix.join(lib_subdir);
                        if library_exists(&lib_dir, lib, &config.lib_patterns) {
                            return Ok(ExternalDependencyInfo::Library {
                                prefix: prefix.clone(),
                                include_dir: inc_dir,
                                lib_dir,
                                lib_name: lib.clone(),
                            });
                        }
                    }
                }

                Err(ExternalDependencyError::NotFound(lib.clone()))
            }
        }
    }
}

fn library_exists(lib_dir: &Path, lib_name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        let file_name = pattern.replace('?', lib_name);
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

fn not_found_error_msg(name: &str) -> String {
    let env_dir = format!("{}_DIR", name.to_uppercase());
    let env_inc = format!("{}_INCDIR", name.to_uppercase());
    let env_lib = format!("{}_LIBDIR", name.to_uppercase());

    format!(
        r#"External dependency not found: {}.
Consider one of the following:
1. Set environment variables:
   - {} for the installation prefix, or
   - {} and {} for specific directories
2. Add the installation prefix to the configuration:
   {} = "/path/to/installation""#,
        name, env_dir, env_inc, env_lib, name
    )
}
