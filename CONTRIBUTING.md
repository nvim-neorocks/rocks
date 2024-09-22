# Contributing guide

Contributions are more than welcome!

## Commit messages / PR title

Please ensure your pull request title conforms to [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).

## CI

Our CI checks are run using [`nix`](https://nixos.org/download.html#download-nix).

## Development

To be able to reproduce CI, we recommend using `nix`.

### Dev environment

We use the following tools:

#### Formatting

- [`rustfmt`](https://github.com/rust-lang/rustfmt) [Rust]
- [`alejandra`](https://github.com/kamadorueda/alejandra) [Nix]

#### Linting

- [`cargo check`](https://doc.rust-lang.org/cargo/commands/cargo-check.html)
- [`clippy`](https://doc.rust-lang.org/clippy/)

### Nix devShell

- Requires [flakes](https://nixos.wiki/wiki/Flakes) to be enabled.

We provide a `flake.nix` that can bootstrap all of the above development tools.

To enter a development shell:

```console
nix develop
```

To apply formatting, while in a devShell, run

```console
pre-commit run --all
```

You can use [`direnv`](https://direnv.net/) to auto-load the development shell.
Just run `direnv allow`.

### Running tests

The easiest way to run tests is with Nix (see below).

If you do not use Nix, you can also run the test suite locally.

### Running tests and checks with Nix

If you just want to run all checks that are available, run:

```console
nix flake check -Lv
```

To run individual checks, using Nix:

```console
nix build .#checks.<your-system>.<check> -Lv
```

For example:

```console
nix build .#checks.x86_64-linux.tests -Lv
```

For formatting and linting:

```console
nix build .#checks.<your-system>.git-hooks-check -Lv
```

## Running tests without nix

For reproducibility, we only run tests that can be sandboxed with nix,
skipping integration tests.
Running `cargo test` locally will run all tests, including integration tests.

Or, if you are using [cargo-nextest](https://nexte.st/), we provide an alias:

```bash
cargo tt
```
