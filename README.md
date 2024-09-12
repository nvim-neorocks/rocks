# A Library & Client implementation of [`luarocks`](https://github.com/luarocks/luarocks)

Rocks serves as an application for:
- Installing and managing rocks
- Creating Lua projects with dependencies, build scripts and desired Lua versions
- Creating and publishing your own rocks
- Embedding rock manipulation logic into your own application

> [!NOTE]
> This aims to be a full rewrite of `luarocks`, with many flags altered to be more
> ergonomic. This is not a drop-in replacement for `luarocks` commands you may have in scripts.

## :books: Usage

```sh
rocks <command> <options>
```

To view available options and their descriptions, run `rocks --help`.

## :book: License

`rocks` is licensed under [MIT](./LICENSE).

## :green_heart: Contributing

Contributions are more than welcome!
See [CONTRIBUTING.md](./CONTRIBUTING.md) for a guide.
