local _MODREV, _SPECREV = 'scm', '-1'
rockspec_format = '3.0'
package = 'sample-project-busted'
version = _MODREV .. _SPECREV

test_dependencies = {
  'lua >= 5.1',
}

source = {
  url = 'https://github.com/nvim-neorocks/luarocks-stub',
}

build = {
  type = 'builtin',
}
