use crate::project::NewProject;
use std::path::PathBuf;

use add::Add;
use build::Build;
use clap::{Parser, Subcommand};
use config::ConfigCmd;
use debug::Debug;
use doc::Doc;
use download::Download;
use info::Info;
use install::Install;
use install_rockspec::InstallRockspec;
use list::ListCmd;
use lux_lib::config::LuaVersion;
use outdated::Outdated;
use pack::Pack;
use path::Path;
use pin::ChangePin;
use remove::Remove;
use run::Run;
use run_lua::RunLua;
use search::Search;
use test::Test;
use uninstall::Uninstall;
use update::Update;
use upload::Upload;
use url::Url;
use which::Which;

pub mod add;
pub mod build;
pub mod check;
pub mod config;
pub mod debug;
pub mod doc;
pub mod download;
pub mod fetch;
pub mod format;
pub mod info;
pub mod install;
pub mod install_lua;
pub mod install_rockspec;
pub mod list;
pub mod outdated;
pub mod pack;
pub mod path;
pub mod pin;
pub mod project;
pub mod purge;
pub mod remove;
pub mod run;
pub mod run_lua;
pub mod search;
pub mod test;
pub mod uninstall;
pub mod unpack;
pub mod update;
pub mod upload;
pub mod utils;
pub mod which;

/// A luxurious package manager for Lua.
#[derive(Parser)]
#[command(author, version, about, long_about = None, arg_required_else_help = true)]
pub struct Cli {
    /// Enable the sub-repositories in luarocks servers for
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

    /// Specify the luarocks server namespace to use.
    #[arg(long, value_name = "namespace")]
    pub namespace: Option<String>,

    /// Specify the luarocks server namespace to use.
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
    /// Add a dependency to the current project.
    Add(Add),
    /// Build/compile a project.
    Build(Build),
    /// Runs `luacheck` in the current project.
    Check,
    /// Interact with the lux configuration.
    #[command(subcommand, arg_required_else_help = true)]
    Config(ConfigCmd),
    /// Various debugging utilities.
    #[command(subcommand, arg_required_else_help = true)]
    Debug(Debug),
    /// [UNIMPLEMENTED] Show documentation for an installed rock.
    Doc(Doc),
    /// Download a specific rock file from a luarocks server.
    #[command(arg_required_else_help = true)]
    Download(Download),
    /// Formats the codebase with stylua.
    Fmt,
    /// Show metadata for any rock.
    Info(Info),
    /// Install a rock for use on the system.
    #[command(arg_required_else_help = true)]
    Install(Install),
    /// Install a local rockspec for use on the system.
    #[command(arg_required_else_help = true)]
    InstallRockspec(InstallRockspec),
    /// Manually install and manage Lua headers for various Lua versions.
    InstallLua,
    /// [UNIMPLEMENTED] Check syntax of a rockspec.
    Lint,
    /// List currently installed rocks.
    List(ListCmd),
    /// Run lua, with the `LUA_PATH` and `LUA_CPATH` set to the specified lux tree.
    Lua(RunLua),
    /// Create a new Lua project.
    New(NewProject),
    /// List outdated rocks.
    Outdated(Outdated),
    /// Create a packed rock for distribution, packing sources or binaries.
    Pack(Pack),
    /// Return the currently configured package path.
    Path(Path),
    /// Pin an existing rock, preventing any updates to the package.
    Pin(ChangePin),
    /// Remove all installed rocks from a tree.
    Purge,
    /// Remove a rock from the current project's lux.toml dependencies.
    Remove(Remove),
    /// Run a command that has been installed with lux.
    /// If the command is not found:
    /// When run from within a lux project, this command will build the project.
    /// Otherwise, it will try to install a package named after the command.
    Run(Run),
    /// Query the luarocks servers.
    #[command(arg_required_else_help = true)]
    Search(Search),
    /// Run the test suite in the current directory.
    Test(Test),
    /// Uninstall a rock from the system.
    Uninstall(Uninstall),
    /// Unpins an existing rock, allowing updates to alter the package.
    Unpin(ChangePin),
    /// Updates all rocks in a project.
    Update(Update),
    /// Upload a rockspec to the public luarocks repository.
    Upload(Upload),
    /// Tell which file corresponds to a given module name.
    Which(Which),
}
