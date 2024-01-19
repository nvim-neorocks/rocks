use std::{path::PathBuf, time::Duration};

use clap::{Parser, Subcommand};
use rocks_lib::config::Config;

mod download;
mod search;

/// An small and efficient Lua package manager.
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

    /// Specifies the cache directory for e.g. luarocks manifests.
    #[arg(long, value_name = "path")]
    cache_path: Option<PathBuf>,

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
    Download(download::Download),
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
    Search(search::Search),
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let config = Config::new()
        .dev(cli.dev)
        .lua_dir(cli.lua_dir)
        .lua_version(cli.lua_version)
        .namespace(cli.namespace)
        .only_server(cli.only_server)
        .only_sources(cli.only_sources)
        .server(cli.server)
        .tree(cli.tree)
        .global(cli.global)
        // .cache_path(cli.cache_path)
        .local(cli.local)
        .timeout(
            cli.timeout
                .map(|duration| Duration::from_secs(duration as u64)),
        )
        .no_project(cli.no_project)
        .verbose(cli.verbose);

    match cli.command {
        Some(command) => match command {
            Commands::Search(search_data) => search::search(search_data, &config).await.unwrap(),
            Commands::Download(download_data) => {
                download::download(download_data, &config).await.unwrap()
            }
            _ => unimplemented!(),
        },
        None => {
            println!("TODO: Display configuration information here. Consider supplying a command instead.");
        }
    }
}
