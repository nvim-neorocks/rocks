# `rocks` rewrite

As the project has grown, the intentions of the project have changed. Attempting to reimplement various parts of luarocks bug-for-bug gives no real benefits - nobody wins.
As such, the goal of `rocks` has since changed. We'd like to be a newer package manager for Lua in the same way `bun` is a better `npm` for Javascript.

`rocks` will continue being compatible with `luarocks.org`, of course, but we aim to greatly improve and simplify the existing workflows for Lua projects.

To achieve this, we seek to refactor various aspects of `luarocks`, including:
- The projects system - the lua project structure is dated and unergonomic, specifically rockspecs and localized `lua` and `luarocks` binaries.
- The rockspec system - we don't plan on changing the rockspec *format* a single bit, but we do plan on making dealing with rockspecs much simpler.
  Instead of managing dozens of different `.rockspec` files for each version of your project, a single rockspec file (project-local) can be used to derive future revisions.
- Removal of `--global` vs `--local` - by default, `rocks` will install dependencies locally for the current user, and globally when executed with `sudo`.
  Either that, or we only keep `--global` as a flag.
- Several Lua versions - certain rocks are built up of C files which access Lua headers. `rocks` will be able to install library headers for various Lua versions automatically,
  without relying on the user to have those present on their system.

# TODOs

## Building

The current build system is functional, but lacks support for compiling Lua C modules.

- [ ] Despite the build system working, `rocks install` (a combination of `rocks unpack-remote` and `rocks build`) is not implemented yet.
- [ ] Support for compiling Lua C modules.
  - [ ] `rocks install-lua` - a command to install headers for a specific Lua version. This command should be invoked automatically when invoking
        `rocks install` on a rock which requires C headers. The command can be used to forcefully use downloaded headers instead of system ones.
        If `rocks` detects system headers for a given Lua version, it won't auto-download its own on an invocation of `rocks install`.
- [ ] `rocks add` - a command different to `rocks install`, whose purpose is to add a dependency to a project. `rocks install`, on the other hand, installs
      a rock for use anywhere (usually a binary rock).

## Lockfiles

`rocks` should have full lockfile support in order to help reproducible dependency management (and also hopefully make Nix-based lua modules easier to use).

Lockfile functionaly will be present by default and will contain information about currently installed rocks as well as the version information of all dependencies.
The lockfile can be modified by using the `rocks lock` command, including:
- `rocks lock update` - update the lockfile by pulling in the latest version of all packages

TODO: Establish more `lock`-based commands, I cannot think of any currently. Having a lock command is beneficial as it prevents the odd Cargo design choice of having `cargo update`
as well as `cargo install-update` (to differentiate between project-local dependencies and global libraries).

## Uploading

Uploading to `luarocks.org` requires a few preliminary factors:
- [Projects](#projects)
- Automatic rockspec creation

After that, full API key support (and integrations with credential managers, perhaps?) should allow for simple uploading to `luarocks.org`.

## Projects

Projects should be entirely overhauled in the new `rocks` rewrite. A single `project.lua` file should be created in the root of the project. This lua file will contain
the template from which traditional rockspecs will be derived when uploading to `luarocks.org`.

Things to establish:
- If the version key is omitted in the `project.lua`, should `rocks` try to read the version from tags/branches? This may have weird consequences.

## Configuration

Lua-based configuration is sensible if the underlying project is written in Lua, but simply proves to be slow anywhere else. For this reason, `rocks` should be configurable using
a TOML file, just like all other modern package managers and CLI projects are (examples: jujutsu, cargo).

The available options in the configuration file will be determined on a per-need basis.
