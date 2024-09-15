use crate::project::write_new::NewProject;
use std::{path::PathBuf, time::Duration};

use build::Build;
use clap::{Parser, Subcommand};
use debug::Debug;
use download::Download;
use install::Install;
use list::ListCmd;
use outdated::Outdated;
use rocks_lib::config::{ConfigBuilder, LuaVersion};
use search::Search;
use update::Update;

mod build;
mod debug;
mod download;
mod format;
mod install;
mod install_lua;
mod list;
mod outdated;
mod project;
mod search;
mod unpack;
mod update;
mod utils;

/// A fast and efficient Lua package manager.
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
    #[arg(long, value_name = "ver")]
    lua_version: Option<LuaVersion>,

    /// Which tree to operate on.
    #[arg(long, value_name = "tree")]
    tree: Option<PathBuf>,

    /// Specifies the cache directory for e.g. luarocks manifests.
    #[arg(long, value_name = "path")]
    cache_path: Option<PathBuf>,

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
    /// Add a dependency to the current project.
    Add,
    /// Build/compile a rock.
    Build(Build),
    /// Query information about Rocks's configuration.
    Config,
    /// Various debugging utilities.
    #[command(subcommand)]
    Debug(Debug),
    /// Show documentation for an installed rock.
    Doc,
    /// Download a specific rock file from a rocks server.
    Download(Download),
    /// Formats the codebase according to a `stylua.toml`.
    Fmt,
    /// Install a rock for use on the system.
    Install(Install),
    /// Manually install and manage Lua headers for various Lua versions.
    InstallLua,
    /// Check syntax of a rockspec.
    Lint,
    /// List currently installed rocks.
    List(ListCmd),
    /// Create a new Lua project.
    New(NewProject),
    /// List outdated rocks.
    Outdated(Outdated),
    /// Create a rock, packing sources or binaries.
    Pack,
    /// Return the currently configured package path.
    Path,
    /// Remove all installed rocks from a tree.
    Purge,
    /// Uninstall a rock.
    Remove,
    /// Query the Luarocks servers.
    Search(Search),
    /// Show information about an installed rock.
    Show,
    /// Run the test suite in the current directory.
    Test,
    /// Uninstall a rock from the system.
    Uninstall,
    /// Updates all rocks in a project.
    Update(Update),
    /// Upload a rockspec to the public rocks repository.
    Upload,
    /// Tell which file corresponds to a given module name.
    Which,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let config = ConfigBuilder::new()
        .dev(Some(cli.dev))
        .lua_dir(cli.lua_dir)
        .lua_version(cli.lua_version)
        .namespace(cli.namespace)
        .only_server(cli.only_server)
        .only_sources(cli.only_sources)
        .server(cli.server)
        .tree(cli.tree)
        .timeout(
            cli.timeout
                .map(|duration| Duration::from_secs(duration as u64)),
        )
        .no_project(Some(cli.no_project))
        .verbose(Some(cli.verbose))
        .build()
        .unwrap();

    match cli.command {
        Some(command) => match command {
            Commands::Search(search_data) => search::search(search_data, config).await.unwrap(),
            Commands::Download(download_data) => {
                download::download(download_data, config).await.unwrap()
            }
            Commands::Debug(debug) => match debug {
                Debug::Unpack(unpack_data) => unpack::unpack(unpack_data).await.unwrap(),
                Debug::UnpackRemote(unpack_data) => {
                    unpack::unpack_remote(unpack_data, config).await.unwrap()
                }
            },
            Commands::New(project_data) => project::write_new::write_project_rockspec(project_data)
                .await
                .unwrap(),
            Commands::Build(build_data) => build::build(build_data, config).unwrap(),
            Commands::List(list_data) => list::list_installed(list_data, config).unwrap(),
            Commands::Install(install_data) => {
                install::install(install_data, config).await.unwrap()
            }
            Commands::Outdated(outdated) => outdated::outdated(outdated, config).await.unwrap(),
            Commands::InstallLua => install_lua::install_lua(config).unwrap(),
            Commands::Fmt => format::format().unwrap(),
            _ => unimplemented!(),
        },
        None => {
            println!("TODO: Display configuration information here. Consider supplying a command instead.");
        }
    }
}
