pub mod install_binary_rock;
pub mod luarocks_installation;
pub mod rock_manifest;

/// Retrieves the target compilation platform and returns it as a luarocks identifier.
pub(crate) fn current_platform_luarocks_identifier() -> String {
    let platform = match std::env::consts::OS {
        "macos" => "macosx",
        p => p,
    };
    format!("{}-{}", platform, std::env::consts::ARCH)
}
