use std::{path::PathBuf, time::Duration};

use build::Build;
use clap::{Parser, Subcommand};
use debug::Debug;
use download::Download;
use list::ListCmd;
use rocks_lib::config::{Config, LuaVersion};
use rockspec::WriteRockspec;
use search::Search;
use update::Update;

mod build;
mod debug;
mod download;
mod list;
mod rockspec;
mod search;
mod unpack;
mod update;

fn parse_lua_version(s: &str) -> Result<LuaVersion, String> {
    match s {
        "5.1" | "51" | "jit" | "luajit" => Ok(LuaVersion::Lua51),
        "5.2" | "52" => Ok(LuaVersion::Lua52),
        "5.3" | "53" => Ok(LuaVersion::Lua53),
        "5.4" | "54" => Ok(LuaVersion::Lua54),
        _ => Err(
            "unrecognized Lua version. Allowed versions: '5.1', '5.2', '5.3', '5.4', 'jit'.".into(),
        ),
    }
}

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
    #[arg(long, value_name = "ver", value_parser = parse_lua_version)]
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
    /// Initialize a directory for a Lua project using Rocks.
    Init,
    /// Install a rock for use on the system.
    Install,
    /// Check syntax of a rockspec.
    Lint,
    /// List currently installed rocks.
    List(ListCmd),
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
    /// Write a template for a rockspec file.
    WriteRockspec(WriteRockspec),
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
        // .cache_path(cli.cache_path)
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
            Commands::Debug(debug) => match debug {
                Debug::Unpack(unpack_data) => unpack::unpack(unpack_data).await.unwrap(),
                Debug::UnpackRemote(unpack_data) => {
                    unpack::unpack_remote(unpack_data, &config).await.unwrap()
                }
            }
            Commands::WriteRockspec(rockspec_data) => {
                rockspec::write_rockspec(rockspec_data).await.unwrap()
            }
            Commands::Build(build_data) => build::build(build_data, &config).unwrap(),
            Commands::List(list_data) => list::list_installed(list_data, &config).unwrap(),
            _ => unimplemented!(),
        },
        None => {
            println!("TODO: Display configuration information here. Consider supplying a command instead.");
        }
    }
}
