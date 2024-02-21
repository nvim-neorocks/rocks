
use crate::rockspec::{Rockspec, Build, BuildBackendSpec};
use eyre::Result;

pub fn build(rockspec: Rockspec, no_install: bool) -> Result<()> {
    // TODO: Ensure dependencies and build dependencies.
    match rockspec.build.default.build_backend.as_ref().cloned() {
        Some(BuildBackendSpec::Builtin(spec)) => spec.run(rockspec, no_install)?,
        _ => unimplemented!(),
    };

    Ok(())
}
