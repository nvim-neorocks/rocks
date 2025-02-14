//! Utilities for converting a list of packages into a list with the correct build behaviour.

use inquire::Confirm;
use lux_lib::{
    build::BuildBehaviour,
    lockfile::PinnedState,
    package::PackageReq,
    tree::{RockMatches, Tree},
};

pub fn apply_build_behaviour(
    package_reqs: Vec<PackageReq>,
    pin: PinnedState,
    force: bool,
    tree: &Tree,
) -> Vec<(BuildBehaviour, PackageReq)> {
    package_reqs
        .into_iter()
        .filter_map(|req| {
            let build_behaviour: Option<BuildBehaviour> = match tree
                .match_rocks_and(&req, |rock| pin == rock.pinned())
                .expect("unable to get tree data")
            {
                RockMatches::Single(_) | RockMatches::Many(_) if !force => {
                    if Confirm::new(&format!("Package {} already exists. Overwrite?", req))
                        .with_default(false)
                        .prompt()
                        .expect("Error prompting for reinstall")
                    {
                        Some(BuildBehaviour::Force)
                    } else {
                        None
                    }
                }
                _ => Some(BuildBehaviour::from(force)),
            };
            build_behaviour.map(|it| (it, req))
        })
        .collect()
}
