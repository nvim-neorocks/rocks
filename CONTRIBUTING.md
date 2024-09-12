<!-- TODO: Section on nix CI + devShell -->

## Running tests locally

For reproducibility, we only run tests that can be sandboxed in CI.
To run tests locally with non-sandboxed tests enabled:

```bash
cargo test --features=test_nosandbox
```

Or, if you are using [cargo-nextest](https://nexte.st/), we provide an alias:

```bash
cargo tt
```
