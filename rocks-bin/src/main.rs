use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Enable the sub-repositories in rocks servers for
    /// rockspecs of in-development versions.
    #[arg(long)]
    dev: bool,

    /// Fetch rocks/rockspecs from this server (takes priority
    /// over config file).
    #[arg(long, value_name = "server")]
    server: Option<String>,

    /// Fetch rocks/rockspecs from this server only (overrides
    /// any entries in the config file).
    #[arg(long, value_name = "server")]
    only_server: Option<String>,

    /// Restrict downloads to paths matching the given URL.
    #[arg(long, value_name = "url")]
    only_sources: Option<String>,

    /// Specify the rocks server namespace to use.
    #[arg(long, value_name = "namespace")]
    namespace: Option<String>,

    /// Specify the rocks server namespace to use.
    #[arg(long, value_name = "prefix")]
    lua_dir: Option<PathBuf>,

    /// Which Lua installation to use.
    // TODO(vhyrro): Add option validator for the version here.
    #[arg(long, value_name = "ver")]
    lua_version: Option<String>,

    /// Which tree to operate on.
    #[arg(long, value_name = "tree")]
    tree: Option<PathBuf>,

    /// Use the tree in the user's home directory.
    /// To enable it, see `rocks help path`.
    #[arg(long)]
    local: bool,

    /// Use the system tree when `local_by_default` is `true`.
    // TODO(vhyrro): Add more insightful description.
    #[arg(long)]
    global: bool,

    /// Do not use project tree even if running from a project folder.
    #[arg(long)]
    no_project: bool,

    /// Display verbose output of commands executed.
    #[arg(long)]
    verbose: bool,

    /// Timeout on network operations, in seconds.
    /// 0 means no timeout (wait forever). Default is 30.
    #[arg(long, value_name = "seconds")]
    timeout: Option<usize>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Build/compile a rock.
    Build,
    /// Query information about Rocks's configuration.
    Config,
    /// Show documentation for an installed rock.
    Doc,
    /// Download a specific rock file from a rocks server.
    Download,
    /// Initialize a directory for a Lua project using Rocks.
    Init,
    /// Install a rock.
    Install,
    /// Check syntax of a rockspec.
    Lint,
    /// List currently installed rocks.
    List,
    /// Compile package in current directory using a rockspec.
    Make,
    /// Auto-write a rockspec for a new version of the rock.
    NewVersion,
    /// Create a rock, packing sources or binaries.
    Pack,
    /// Return the currently configured package path.
    Path,
    /// Remove all installed rocks from a tree.
    Purge,
    /// Uninstall a rock.
    Remove,
    /// Query the Luarocks servers.
    Search,
    /// Show information about an installed rock.
    Show,
    /// Run the test suite in the current directory.
    Test,
    /// Unpack the contents of a rock.
    Unpack,
    /// Upload a rockspec to the public rocks repository.
    Upload,
    /// Tell which file corresponds to a given module name.
    Which,
    /// Write a template for a rockspec file.
    WriteRockspec,
}

fn main() {
    let _cli = Cli::parse();
}