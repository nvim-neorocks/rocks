package = "make-project"
version = "scm-1"

source = {
    url = 'file://resources/test/make-project',
}

build = {
  type = "make",
  build_variables = {
     LUA_LIBDIR="$(LUA_LIBDIR)",
     LUA_INCDIR="$(LUA_INCDIR)",
  },
  install_variables = {
     INST_PREFIX="$(PREFIX)",
     INST_BINDIR="$(BINDIR)",
     INST_LIBDIR="$(LIBDIR)",
     INST_LUADIR="$(LUADIR)",
     INST_CONFDIR="$(CONFDIR)",
  },
}
