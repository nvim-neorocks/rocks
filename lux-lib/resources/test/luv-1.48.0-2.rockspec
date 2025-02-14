package = "luv"
version = "1.48.0-2"
source = {
  url = 'https://github.com/luvit/luv/releases/download/'..version..'/luv-'..version..'.tar.gz'
}
rockspec_format = "3.0"

dependencies = {
  "lua >= 5.1"
}

build = {
  type = 'cmake',
  variables = {
     CMAKE_C_FLAGS="$(CFLAGS)",
     CMAKE_MODULE_LINKER_FLAGS="$(LIBFLAG)",
     LUA_LIBDIR="$(LUA_LIBDIR)",
     LUA_INCDIR="$(LUA_INCDIR)",
     LUA_LIBFILE="$(LUALIB)",
     LUA="$(LUA)",
     LIBDIR="$(LIBDIR)",
     LUADIR="$(LUADIR)",
  },
}
