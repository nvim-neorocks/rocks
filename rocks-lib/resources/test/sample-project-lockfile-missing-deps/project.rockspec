rockspec_format = "3.0"
package = "foo"
version = "1.0.0-1"

dependencies = {
    "lua >= 5.1",
    "pathlib.nvim == 2.2.3",
    -- "rtp.nvim == 1.2.0",
}

source = {
  url = 'https://github.com/nvim-neorocks/luarocks-stub',
}
