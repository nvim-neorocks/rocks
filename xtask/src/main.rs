use std::{
    env,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use clap::{CommandFactory, ValueEnum};
use clap_complete::{generate_to, Shell};
use clap_mangen::Man;
use lux_cli::Cli;

type DynError = Box<dyn std::error::Error>;

fn main() {
    if let Err(e) = try_main() {
        eprintln!("{}", e);
        std::process::exit(-1);
    }
}

fn try_main() -> Result<(), DynError> {
    let task = env::args().nth(1);

    match task.as_deref() {
        // Assume that the user wants to build the release version
        // when trying to build the distributed version.
        Some("dist") => dist(true)?,
        Some("dist-man") => dist_man()?,
        Some("dist-completions") => dist_completions()?,
        Some("build") => build(false)?,
        Some("build-release") => build(true)?,
        _ => print_help(),
    }

    Ok(())
}

fn print_help() {
    eprintln!(
        "Tasks:

build               builds and links all libraries and the application
dist-man            builds man pages
dist-completions    builds shell completions
dist                builds everything, equivalent to build + dist-man + dist-completions

Environment variables:
LUA_LIB_DIR         when set, overrides the path to the directory containing the compiled lux-lua libraries
"
    )
}

fn dist(release: bool) -> Result<(), DynError> {
    build(release)?;
    dist_man()?;
    dist_completions()
}

fn build(release: bool) -> Result<(), DynError> {
    let _ = fs::remove_dir_all(dist_dir());
    fs::create_dir_all(dist_dir())?;

    let profile = if release { "release" } else { "debug" };

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let dest_dir = project_root().join(format!("target/{profile}"));

    // Build all variants of the lux-lua library
    for (lua_feature_flag, canonical_version) in [
        ("lua51", "5.1"),
        ("lua52", "5.2"),
        ("lua53", "5.3"),
        ("lua54", "5.4"),
    ] {
        let mut args = vec!["build", "--features", lua_feature_flag];

        if release {
            args.push("--release");
        }

        let status = Command::new(&cargo)
            .current_dir(project_root().join("lux-lua"))
            .args(args)
            .status()?;

        if !status.success() {
            Err("cargo build failed")?;
        }

        let dir = if release {
            dist_dir()
        } else {
            dest_dir.clone()
        };

        let _ = fs::remove_dir_all(dir.join(canonical_version));
        fs::create_dir_all(dir.join(canonical_version))?;

        fs::copy(
            project_root().join(format!("lux-lua/target/{profile}/liblux_lua.so")),
            dir.join(format!("{canonical_version}/lux.so")),
        )?;
    }

    let mut args = vec!["build", "--features", "luajit"];

    if release {
        args.push("--release");
    }

    // Build with luajit by default.
    let status = Command::new(cargo)
        .current_dir(project_root())
        .args(args)
        .env(
            "LUX_LIB_DIR",
            env::var("LUX_LIB_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    if release {
                        dist_dir()
                    } else {
                        dest_dir.clone()
                    }
                }),
        )
        .status()?;

    if !status.success() {
        Err("cargo build failed")?;
    }

    let dest = dest_dir.join("lx");

    if release {
        fs::copy(&dest, dist_dir().join("lx"))?;
    }

    if release
        && Command::new("strip")
            .arg("--version")
            .stdout(Stdio::null())
            .status()
            .inspect_err(|_| eprintln!("checking for `strip` utility"))
            .is_ok()
    {
        eprintln!("stripping the binary");
        let status = Command::new("strip").arg(&dest).status()?;
        if !status.success() {
            Err("strip failed")?;
        }
    }

    Ok(())
}

fn dist_man() -> Result<(), DynError> {
    fs::create_dir_all(dist_dir())?;

    let cmd = &mut Cli::command();

    Man::new(cmd.clone())
        .render(&mut File::create(dist_dir().join("lux.1")).unwrap())
        .unwrap();
    Ok(())
}

fn dist_completions() -> Result<(), DynError> {
    fs::create_dir_all(dist_dir())?;

    let cmd = &mut Cli::command();

    for shell in Shell::value_variants() {
        generate_to(*shell, cmd, "lx", dist_dir()).unwrap();
    }

    Ok(())
}

fn project_root() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf()
}

fn dist_dir() -> PathBuf {
    project_root().join("target/dist")
}
