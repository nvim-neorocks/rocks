
use crate::rockspec::{Rockspec, Build};
use eyre::Result;

pub fn build(rockspec: Rockspec, no_install: bool) -> Result<()> {
    // TODO: Ensure dependencies and build dependencies.
    match rockspec.build.default.build_backend.unwrap() {
        crate::rockspec::BuildBackendSpec::Builtin(spec) => spec.run(no_install)?,
        _ => unimplemented!(),
    };

    Ok(())
}
