# A [WIP] Library & Client implementation of [`luarocks`](https://github.com/luarocks/luarocks)

> [!WARNING]
>
> **rocks is a work in progress
> and does not have a stable release yet.**

Rocks serves as an application for:
- Installing and managing rocks
- Creating Lua projects with dependencies, build scripts and desired Lua versions
- Creating and publishing your own rocks
- Embedding rock manipulation logic into your own application

> [!NOTE]
>
> This aims to be a full rewrite of `luarocks`, with many flags altered to be more
> ergonomic. This is not a drop-in replacement for `luarocks` commands you may have in scripts.

## :books: Usage

```sh
rocks <command> <options>
```

To view available options and their descriptions, run `rocks --help`.

## Comparison with [`luarocks v3.11.1`](https://github.com/luarocks/luarocks)

As this project is still a work in progress, some luarocks features
have not been (fully) implemented yet.
On the other hand, rocks has some features that are not present in luarocks.

The following table provides a brief (incomplete) comparison:

|                                                                       | `rocks`                      | `luarocks v3.11.1` |
| ---                                                                   | ---                          | ---                |
| `builtin` build spec                                                  | :white_check_mark:           | :white_check_mark: |
| `make` build spec                                                     | :white_check_mark:           | :white_check_mark: |
| `cmake` build spec                                                    | :white_check_mark:           | :white_check_mark: |
| `command` build spec                                                  | :white_check_mark:           | :white_check_mark: |
| custom build backends                                                 | :white_check_mark:[^1]       | :white_check_mark: |
| `rust-mlua` build spec                                                | :white_check_mark: (builtin) | :white_check_mark: (external build backend) |
| install pre-built binary rocks                                        | :white_check_mark:           | :white_check_mark: |
| parallel builds/installs                                              | :white_check_mark:           | :x:                |
| install multiple packages with a single command                       | :white_check_mark:           | :x:                |
| install packages using version constraints                            | :white_check_mark:           | :x:                |
| proper lockfile support with integrity checks                         | :white_check_mark:           | :x: (basic, dependency versions only) |
| auto-detect external dependencies and Lua headers with `pkg-config`   | :white_check_mark:           | :x:                |
| automatic lua detection/installation                                  | :white_check_mark:           | :x:                |
| resolve multiple versions of the same dependency at runtime           | :x: (planned)                | :white_check_mark: |
| run tests with busted                                                 | :white_check_mark:           | :white_check_mark: |
| code formatting with stylua                                           | :white_check_mark:           | :x:                |
| linting with luacheck                                                 | :white_check_mark:           | :x:                |
| static type checking                                                  | :x: (planned)                | :x:                |
| pack and upload pre-built binary rocks                                | :white_check_mark:           | :white_check_mark: |
| add/remove dependencies                                               | :white_check_mark:           | :x:                |
| luarocks.org manifest namespaces                                      | :x: (planned)                | :white_check_mark: |
| luarocks.org dev packages                                             | :x: (planned)                | :white_check_mark: |

[^1]: Supported via a compatibility layer that uses luarocks as a backend.

## :book: License

`rocks` is licensed under [MIT](./LICENSE).

## :green_heart: Contributing

Contributions are more than welcome!
See [CONTRIBUTING.md](./CONTRIBUTING.md) for a guide.
