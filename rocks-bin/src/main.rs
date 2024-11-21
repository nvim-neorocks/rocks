use crate::project::write_new::NewProject;
use std::{path::PathBuf, time::Duration};

use build::Build;
use clap::{Parser, Subcommand};
use debug::Debug;
use download::Download;
use info::Info;
use install::Install;
use list::ListCmd;
use outdated::Outdated;
use path::Path;
use pin::ChangePin;
use remove::Remove;
use rocks_lib::{
    config::{ConfigBuilder, LuaVersion},
    lockfile::PinnedState::{Pinned, Unpinned},
};
use run::Run;
use run_lua::RunLua;
use search::Search;
use test::Test;
use update::Update;
use upload::Upload;

mod build;
mod debug;
mod download;
mod fetch;
mod format;
mod info;
mod install;
mod install_lua;
mod list;
mod outdated;
mod path;
mod pin;
mod project;
mod purge;
mod remove;
mod run;
mod run_lua;
mod search;
mod test;
mod unpack;
mod update;
mod upload;
mod utils;

/// A fast and efficient Lua package manager.
#[derive(Parser)]
#[command(author, version, about, long_about = None, arg_required_else_help = true)]
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
    command: Commands,
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
    #[command(subcommand, arg_required_else_help = true)]
    Debug(Debug),
    /// Show documentation for an installed rock.
    Doc,
    /// Download a specific rock file from a rocks server.
    #[command(arg_required_else_help = true)]
    Download(Download),
    /// Formats the codebase with stylua.
    Fmt,
    /// Show metadata for any rock.
    Info(Info),
    /// Install a rock for use on the system.
    #[command(arg_required_else_help = true)]
    Install(Install),
    /// Manually install and manage Lua headers for various Lua versions.
    InstallLua,
    /// Check syntax of a rockspec.
    Lint,
    /// List currently installed rocks.
    List(ListCmd),
    /// Run lua, with the `LUA_PATH` and `LUA_CPATH` set to the specified rocks tree.
    Lua(RunLua),
    /// Create a new Lua project.
    New(NewProject),
    /// List outdated rocks.
    Outdated(Outdated),
    /// Create a rock, packing sources or binaries.
    Pack,
    /// Return the currently configured package path.
    Path(Path),
    /// Pin an existing rock, preventing any updates to the package.
    Pin(ChangePin),
    /// Remove all installed rocks from a tree.
    Purge,
    /// Uninstall a rock.
    Remove(Remove),
    /// Run a command that has been installed with rocks.
    /// If the command is not found:
    /// When run from within a rocks project, this command will build the project.
    /// Otherwise, it will try to install a package named after the command.
    Run(Run),
    /// Query the Luarocks servers.
    #[command(arg_required_else_help = true)]
    Search(Search),
    /// Run the test suite in the current directory.
    Test(Test),
    /// Uninstall a rock from the system.
    Uninstall,
    /// Unpins an existing rock, allowing updates to alter the package.
    Unpin(ChangePin),
    /// Updates all rocks in a project.
    Update(Update),
    /// Upload a rockspec to the public rocks repository.
    Upload(Upload),
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
        Commands::Search(search_data) => search::search(search_data, config).await.unwrap(),
        Commands::Download(download_data) => {
            download::download(download_data, config).await.unwrap()
        }
        Commands::Debug(debug) => match debug {
            Debug::FetchRemote(unpack_data) => {
                fetch::fetch_remote(unpack_data, config).await.unwrap()
            }
            Debug::Unpack(unpack_data) => unpack::unpack(unpack_data).await.unwrap(),
            Debug::UnpackRemote(unpack_data) => {
                unpack::unpack_remote(unpack_data, config).await.unwrap()
            }
        },
        Commands::New(project_data) => project::write_new::write_project_rockspec(project_data)
            .await
            .unwrap(),
        Commands::Build(build_data) => build::build(build_data, config).await.unwrap(),
        Commands::List(list_data) => list::list_installed(list_data, config).unwrap(),
        Commands::Lua(run_lua) => run_lua::run_lua(run_lua, config).await.unwrap(),
        Commands::Install(install_data) => install::install(install_data, config).await.unwrap(),
        Commands::Outdated(outdated) => outdated::outdated(outdated, config).await.unwrap(),
        Commands::InstallLua => install_lua::install_lua(config).await.unwrap(),
        Commands::Fmt => format::format().unwrap(),
        Commands::Purge => purge::purge(config).await.unwrap(),
        Commands::Remove(remove_args) => remove::remove(remove_args, config).await.unwrap(),
        Commands::Run(run_args) => run::run(run_args, config).await.unwrap(),
        Commands::Test(test) => test::test(test, config).await.unwrap(),
        Commands::Update(_update_args) => update::update(config).await.unwrap(),
        Commands::Info(info_data) => info::info(info_data, config).await.unwrap(),
        Commands::Path(path_data) => path::path(path_data, config).await.unwrap(),
        Commands::Pin(pin_data) => pin::set_pinned_state(pin_data, config, Pinned).unwrap(),
        Commands::Unpin(pin_data) => pin::set_pinned_state(pin_data, config, Unpinned).unwrap(),
        Commands::Upload(upload_data) => upload::upload(upload_data, config).await.unwrap(),
        _ => unimplemented!(),
    }
}
