use std::{path::PathBuf, time::Duration};

use clap::{Parser, Subcommand};
use rocks::{
    build::{self, Build},
    check,
    debug::Debug,
    download::{self, Download},
    fetch, format,
    info::{self, Info},
    install::{self, Install},
    install_lua,
    list::{self, ListCmd},
    outdated::{self, Outdated},
    path::{self, Path},
    pin::{self, ChangePin},
    project::{self, NewProject},
    purge,
    remove::{self, Remove},
    run::{self, Run},
    run_lua::{self, RunLua},
    search::{self, Search},
    test::{self, Test},
    unpack,
    update::{self, Update},
    upload::{self, Upload},
};
use rocks_lib::{
    config::{ConfigBuilder, LuaVersion},
    lockfile::PinnedState::{Pinned, Unpinned},
};
use url::Url;

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
    /// Build/compile a rock.
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

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let cli = Cli::parse();

    let config = ConfigBuilder::new()
        .dev(Some(cli.dev))
        .lua_dir(cli.lua_dir)
        .lua_version(cli.lua_version)
        .namespace(cli.namespace)
        .extra_servers(cli.extra_servers)
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
            Debug::Project => project::debug_project().unwrap(),
        },
        Commands::New(project_data) => project::write_project_rockspec(project_data).await.unwrap(),
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
        Commands::Check => check::check(config).await.unwrap(),
        Commands::Add => unimplemented!(),
        Commands::Config => unimplemented!(),
        Commands::Doc => unimplemented!(),
        Commands::Lint => unimplemented!(),
        Commands::Pack => unimplemented!(),
        Commands::Uninstall => unimplemented!(),
        Commands::Which => unimplemented!(),
    }
}
