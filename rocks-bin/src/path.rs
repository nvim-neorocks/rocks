use std::{env, str::FromStr as _};

use clap::Subcommand;
use eyre::Result;
use rocks_lib::{
    config::{Config, LuaVersion},
    path::{BinPath, PackagePath, Paths},
};
use strum::{EnumString, VariantNames};
use strum_macros::Display;

use clap::{Args, ValueEnum};

#[derive(Args)]
pub struct Path {
    #[command(subcommand)]
    cmd: Option<PathCmd>,

    /// Prepend the rocks tree paths to the system paths.
    #[clap(default_value_t = false)]
    #[arg(long)]
    prepend: bool,
}

#[derive(Subcommand, PartialEq, Eq, Debug, Clone)]
#[clap(rename_all = "kebab_case")]
enum PathCmd {
    /// Generate an export statement for all paths.
    /// (formatted as a shell command) [Default]
    Full(FullArgs),
    /// Generate a `LUA_PATH` expression for `lua` libraries in the rocks tree.
    /// (not formatted as a shell command)
    Lua,
    /// Generate a `LUA_CPATH` expression for native `lib` libraries in the rocks tree.
    /// (not formatted as a shell command)
    C,
    /// Generate a `PATH` expression for `bin` executables in the rocks tree.
    /// (not formatted as a shell command)
    Bin,
}

impl Default for PathCmd {
    fn default() -> Self {
        Self::Full(FullArgs::default())
    }
}

#[derive(Args, PartialEq, Eq, Debug, Clone, Default)]
struct FullArgs {
    /// Do not export `PATH` (`bin` paths).
    #[clap(default_value_t = false)]
    #[arg(long)]
    no_bin: bool,

    /// The shell to format for.
    #[clap(default_value_t = Shell::default())]
    #[arg(long)]
    shell: Shell,
}

#[derive(EnumString, VariantNames, Display, ValueEnum, PartialEq, Eq, Debug, Clone)]
#[strum(serialize_all = "lowercase")]
enum Shell {
    Posix,
    Fish,
    Nu,
}

impl Default for Shell {
    fn default() -> Self {
        Self::Posix
    }
}

pub async fn path(path_data: Path, config: Config) -> Result<()> {
    let tree = config.tree(LuaVersion::from(&config)?)?;
    let paths = Paths::new(tree)?;
    let cmd = path_data.cmd.unwrap_or_default();
    let prepend = path_data.prepend;
    match cmd {
        PathCmd::Full(args) => {
            let mut result = String::new();
            let shell = args.shell;
            let package_path = mk_package_path(&paths, prepend)?;
            if !package_path.is_empty() {
                result.push_str(format_export(&shell, "LUA_PATH", &package_path).as_str());
                result.push('\n')
            }
            let package_cpath = mk_package_cpath(&paths, prepend)?;
            if !package_cpath.is_empty() {
                result.push_str(format_export(&shell, "LUA_CPATH", &package_cpath).as_str());
                result.push('\n')
            }
            if !args.no_bin {
                let path = mk_bin_path(&paths, prepend)?;
                if !path.is_empty() {
                    result.push_str(format_export(&shell, "PATH", &path).as_str());
                    result.push('\n')
                }
            }
            println!("{}", &result);
        }
        PathCmd::Lua => println!("{}", &mk_package_path(&paths, prepend)?),
        PathCmd::C => println!("{}", &mk_package_cpath(&paths, prepend)?),
        PathCmd::Bin => println!("{}", &mk_bin_path(&paths, prepend)?),
    }
    Ok(())
}

fn mk_package_path(paths: &Paths, prepend: bool) -> Result<PackagePath> {
    let mut result = if prepend {
        PackagePath::from_str(env::var("LUA_PATH").unwrap_or_default().as_str()).unwrap_or_default()
    } else {
        PackagePath::default()
    };
    result.prepend(paths.package_path());
    Ok(result)
}

fn mk_package_cpath(paths: &Paths, prepend: bool) -> Result<PackagePath> {
    let mut result = if prepend {
        PackagePath::from_str(env::var("LUA_CPATH").unwrap_or_default().as_str())
            .unwrap_or_default()
    } else {
        PackagePath::default()
    };
    result.prepend(paths.package_cpath());
    Ok(result)
}

fn mk_bin_path(paths: &Paths, prepend: bool) -> Result<BinPath> {
    let mut result = if prepend {
        BinPath::from_env()
    } else {
        BinPath::default()
    };
    result.prepend(paths.path());
    Ok(result)
}

fn format_export<D>(shell: &Shell, var_name: &str, var: &D) -> String
where
    D: std::fmt::Display,
{
    match shell {
        Shell::Posix => format!("export {}='{}';", var_name, var),
        Shell::Fish => format!("set -x {} \"{}\";", var_name, var),
        Shell::Nu => format!("$env.{} = \"{}\";", var_name, var),
    }
}
