use std::{
    env,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use clap::{CommandFactory, ValueEnum};
use clap_complete::{generate_to, Shell};
use clap_mangen::Man;
use rocks::Cli;

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
        Some("dist") => dist()?,
        Some("dist-man") => dist_man()?,
        Some("dist-completions") => dist_completions()?,
        _ => print_help(),
    }
    Ok(())
}

fn print_help() {
    eprintln!(
        "Tasks:

dist                builds application, shell completions and man pages
dist-man            builds man pages
dist-completions    builds shell completions
"
    )
}

fn dist() -> Result<(), DynError> {
    let _ = fs::remove_dir_all(dist_dir());
    fs::create_dir_all(dist_dir())?;

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .current_dir(project_root())
        .args(["build", "--release"])
        .status()?;

    if !status.success() {
        Err("cargo build failed")?;
    }

    let dst = project_root().join("target/release/rocks");

    fs::copy(&dst, dist_dir().join("rocks"))?;

    if Command::new("strip")
        .arg("--version")
        .stdout(Stdio::null())
        .status()
        .is_ok()
    {
        eprintln!("stripping the binary");
        let status = Command::new("strip").arg(&dst).status()?;
        if !status.success() {
            Err("strip failed")?;
        }
    } else {
        eprintln!("no `strip` utility found")
    }
    dist_man()?;
    dist_completions()
}

fn dist_man() -> Result<(), DynError> {
    fs::create_dir_all(dist_dir())?;

    let cmd = &mut Cli::command();

    Man::new(cmd.clone())
        .render(&mut File::create(dist_dir().join("rocks.1")).unwrap())
        .unwrap();
    Ok(())
}

fn dist_completions() -> Result<(), DynError> {
    fs::create_dir_all(dist_dir())?;

    let cmd = &mut Cli::command();

    for shell in Shell::value_variants() {
        generate_to(*shell, cmd, "rocks", dist_dir()).unwrap();
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
