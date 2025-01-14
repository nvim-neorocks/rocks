use std::time::Duration;

use clap::Parser;
use rocks::{
    build, check,
    debug::Debug,
    doc, download, fetch, format, info, install, install_lua, install_rockspec, list, outdated,
    path, pin, project, purge, remove, run, run_lua, search, test, uninstall, unpack, update,
    upload::{self},
    Cli, Commands,
};
use rocks_lib::{
    config::ConfigBuilder,
    lockfile::PinnedState::{Pinned, Unpinned},
};

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
        Commands::InstallRockspec(install_data) => {
            install_rockspec::install_rockspec(install_data, config)
                .await
                .unwrap()
        }
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
        Commands::Doc(doc_args) => doc::doc(doc_args, config).await.unwrap(),
        Commands::Lint => unimplemented!(),
        Commands::Pack => unimplemented!(),
        Commands::Uninstall(uninstall_data) => {
            uninstall::uninstall(uninstall_data, config).await.unwrap()
        }
        Commands::Which => unimplemented!(),
    }
}
