use std::{collections::HashMap, path::PathBuf};

/// Used as a fallback when searching for external dependencies if they
/// cannot be found using pkg-config.
#[derive(Debug, Clone)]
pub struct ExternalDependencySearchConfig {
    /// Patterns for binary files
    pub bin_patterns: Vec<String>,
    /// Patterns for header files
    pub include_patterns: Vec<String>,
    /// Patterns for library files
    pub lib_patterns: Vec<String>,
    /// Default binary subdirectory
    pub bin_subdir: String,
    /// Default include subdirectory
    pub include_subdir: String,
    /// Default library subdirectory
    pub lib_subdir: Vec<String>,
    /// System-wide search paths
    pub search_prefixes: Vec<PathBuf>,
    /// Known installation prefixes for specific dependencies.
    /// These can also be set via environment variables.
    pub prefixes: HashMap<String, PathBuf>,
}

impl Default for ExternalDependencySearchConfig {
    fn default() -> Self {
        Self {
            bin_patterns: vec!["?".into()],
            include_patterns: vec!["?.h".into()],
            lib_patterns: default_lib_patterns(),
            bin_subdir: "bin".into(),
            include_subdir: "include".into(),
            lib_subdir: vec!["lib".into(), "lib64".into()],
            search_prefixes: default_prefixes(),
            prefixes: HashMap::default(),
        }
    }
}

#[cfg(target_family = "unix")]
fn default_lib_patterns() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        vec!["lib?.dylib".to_string(), "lib?.a".to_string()]
    }

    #[cfg(not(target_os = "macos"))]
    {
        vec!["lib?.so".to_string(), "lib?.a".to_string()]
    }
}

#[cfg(target_family = "windows")]
fn default_lib_patterns() -> Vec<String> {
    vec!["?.dll".to_string(), "?.lib".to_string()]
}

#[cfg(target_family = "unix")]
fn default_prefixes() -> Vec<PathBuf> {
    use std::path::PathBuf;

    #[cfg(target_os = "macos")]
    {
        vec![
            PathBuf::from("/usr"),
            PathBuf::from("/usr/local"),
            PathBuf::from("/opt/local"),
            PathBuf::from("/opt/homebrew"),
            PathBuf::from("/opt"),
        ]
    }

    #[cfg(not(target_os = "macos"))]
    {
        vec![
            PathBuf::from("/usr"),
            PathBuf::from("/usr/local"),
            PathBuf::from("/opt/local"),
            PathBuf::from("/opt"),
        ]
    }
}

#[cfg(target_family = "windows")]
fn default_prefixes() -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"C:\Program Files"),
        PathBuf::from(r"C:\Program Files (x86)"),
    ]
}
