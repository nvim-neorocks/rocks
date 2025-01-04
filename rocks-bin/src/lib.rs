use crate::project::NewProject;
use std::path::PathBuf;

use build::Build;
use clap::{Parser, Subcommand};
use debug::Debug;
use download::Download;
use info::Info;
use install::Install;
use install_rockspec::InstallRockspec;
use list::ListCmd;
use outdated::Outdated;
use path::Path;
use pin::ChangePin;
use remove::Remove;
use rocks_lib::config::LuaVersion;
use run::Run;
use run_lua::RunLua;
use search::Search;
use test::Test;
use update::Update;
use upload::Upload;
use url::Url;

pub mod build;
pub mod check;
pub mod debug;
pub mod download;
pub mod fetch;
pub mod format;
pub mod info;
pub mod install;
pub mod install_lua;
pub mod install_rockspec;
pub mod list;
pub mod outdated;
pub mod path;
pub mod pin;
pub mod project;
pub mod purge;
pub mod remove;
pub mod run;
pub mod run_lua;
pub mod search;
pub mod test;
pub mod unpack;
pub mod update;
pub mod upload;
pub mod utils;

/// A fast and efficient Lua package manager.
#[derive(Parser)]
#[command(author, version, about, long_about = None, arg_required_else_help = true)]
pub struct Cli {
    /// Enable the sub-repositories in rocks servers for
    /// rockspecs of in-development versions.
    #[arg(long)]
    pub dev: bool,

    /// Fetch rocks/rockspecs from this server (takes priority
    /// over config file).
    #[arg(long, value_name = "server")]
    pub server: Option<Url>,

    /// Fetch rocks/rockspecs from this server in addition to the main server
    /// (overrides any entries in the config file).
    #[arg(long, value_name = "extra-server")]
    pub extra_servers: Option<Vec<Url>>,

    /// Restrict downloads to paths matching the given URL.
    #[arg(long, value_name = "url")]
    pub only_sources: Option<String>,

    /// Specify the rocks server namespace to use.
    #[arg(long, value_name = "namespace")]
    pub namespace: Option<String>,

    /// Specify the rocks server namespace to use.
    #[arg(long, value_name = "prefix")]
    pub lua_dir: Option<PathBuf>,

    /// Which Lua installation to use.
    #[arg(long, value_name = "ver")]
    pub lua_version: Option<LuaVersion>,

    /// Which tree to operate on.
    #[arg(long, value_name = "tree")]
    pub tree: Option<PathBuf>,

    /// Specifies the cache directory for e.g. luarocks manifests.
    #[arg(long, value_name = "path")]
    pub cache_path: Option<PathBuf>,

    /// Do not use project tree even if running from a project folder.
    #[arg(long)]
    pub no_project: bool,

    /// Display verbose output of commands executed.
    #[arg(long)]
    pub verbose: bool,

    /// Timeout on network operations, in seconds.
    /// 0 means no timeout (wait forever). Default is 30.
    #[arg(long, value_name = "seconds")]
    pub timeout: Option<usize>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// [UNIMPLEMENTED] Add a dependency to the current project.
    Add,
    /// Build/compile a project.
    Build(Build),
    /// Runs `luacheck` in the current project.
    Check,
    /// [UNIMPLEMENTED] Query information about Rocks's configuration.
    Config,
    /// Various debugging utilities.
    #[command(subcommand, arg_required_else_help = true)]
    Debug(Debug),
    /// [UNIMPLEMENTED] Show documentation for an installed rock.
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
    /// Install a local RockSpec for use on the system.
    #[command(arg_required_else_help = true)]
    InstallRockspec(InstallRockspec),
    /// Manually install and manage Lua headers for various Lua versions.
    InstallLua,
    /// [UNIMPLEMENTED] Check syntax of a rockspec.
    Lint,
    /// List currently installed rocks.
    List(ListCmd),
    /// Run lua, with the `LUA_PATH` and `LUA_CPATH` set to the specified rocks tree.
    Lua(RunLua),
    /// Create a new Lua project.
    New(NewProject),
    /// List outdated rocks.
    Outdated(Outdated),
    /// [UNIMPLEMENTED] Create a rock, packing sources or binaries.
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
    /// [UNIMPLEMENTED] Uninstall a rock from the system.
    Uninstall,
    /// Unpins an existing rock, allowing updates to alter the package.
    Unpin(ChangePin),
    /// Updates all rocks in a project.
    Update(Update),
    /// Upload a rockspec to the public rocks repository.
    Upload(Upload),
    /// [UNIMPLEMENTED] Tell which file corresponds to a given module name.
    Which,
}
