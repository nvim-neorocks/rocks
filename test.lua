-- Temporary test file to showcase the expected API

local lux = require("lux")

local config = lux.config.default()
local project = lux.project.current()
local tree = project:tree()

local toml = project:toml():into_local()

lux.build
    :new(toml, tree, config)
    :pin(true)
    :build()
