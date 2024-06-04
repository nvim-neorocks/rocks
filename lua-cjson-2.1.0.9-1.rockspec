package = "lua-cjson"
version = "2.1.0.9-1"

source = {
    url = "git+https://github.com/openresty/lua-cjson",
    -- tag = "2.1.0.9",
}

description = {
    summary = "A fast JSON encoding/parsing module",
    detailed = [[
        The Lua CJSON module provides JSON support for Lua. It features:
        - Fast, standards compliant encoding/parsing routines
        - Full support for JSON with UTF-8, including decoding surrogate pairs
        - Optional run-time support for common exceptions to the JSON specification
          (infinity, NaN,..)
        - No dependencies on other libraries
    ]],
    homepage = "http://www.kyne.com.au/~mark/software/lua-cjson.php",
    license = "MIT"
}

dependencies = {
    "lua >= 5.1"
}

build = {
    type = "builtin",
    modules = {
        cjson = {
            "lua_cjson.c", "strbuf.c", "fpconv.c",
        }
    },
    install = {
        lua = {
            ["cjson.util"] = "lua/cjson/util.lua"
        },
        bin = {
            json2lua = "lua/json2lua.lua",
            lua2json = "lua/lua2json.lua"
        }
    },
    copy_directories = { "tests" }
}

-- vi:ai et sw=4 ts=4:
