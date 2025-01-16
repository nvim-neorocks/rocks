-- A Lua source file packer.
-- released under the Romantic WTF Public License
setmetatable( _G or _ENV, {__index = require"init" } )

args = {...}
--[[]] pcall( require,"luarocks.loader" )
--[[]] local success, fs = pcall( require,"lfs" )
files = {}

fs = success and fs or {
    dir = function (path)
        local listing = io.popen ( "ls "..path ):read"*all"
        local files = {}
        for file in listing:gmatch( "[^\n]+" ) do
            files[file] = true
        end
        return next, files
    end,
   attributes = function() return{} end
}

root = "./"

function scandir (root)
-- adapted from http://keplerproject.github.com/luafilesystem/examples.html
    path = path or ""
    for f in fs.dir( root ) do
        if f:find"%.lua$" then
            hndl = f:gsub( "%.lua$", "" ):gsub("^[/\\]","")
                                         :gsub( "/", "." )
                                         :gsub( "\\", "." )
            table.insert(files, { hndl,  io.open( root..f ):read"*a" })
        end
    end
end

scandir( root )

-- Sort files by name so output is deterministic
table.sort(files, function(a, b) return a[1] < b[1] end)

acc={(io.open("../ABOUT"):read("*all").."\n"):gsub( "([^\n]-\n)","-- %1" ),[[
local _ENV,       loaded, packages, release, require_
    = _ENV or _G, {},     {},       true,    require

local function require(...)
    local lib = ...

    -- is it a private file?
    if loaded[lib] then
        return loaded[lib]
    elseif packages[lib] then
        loaded[lib] = packages[lib](lib)
        return loaded[lib]
    else
        return require_(lib)
    end
end

]]
} local wrapper = { [[
--=============================================================================
do local _ENV = _ENV
packages[']], nil, [['] = function (...)
]], nil, [[

end
end
]] }

local blank = B"\n"*P" "^0 * '\n' /""
local comment = B"\n" * P" "^0 * "--" * (1-P"\n")^0 * "\n" / ""
local strip = Cs((blank + comment + (1-(blank+comment)))^0)

for _,v in ipairs( files ) do
    wrapper[2], wrapper[4] = v[1], strip:match(v[2])
    acc[#acc+1]= table.concat(wrapper)
end

acc[#acc + 1] = [[
return require"init"



]]

acc[#acc+1] = io.open("../LICENSE"):read("*all".."\n"):gsub("([^\n]-\n)","-- %1")

print( table.concat( acc ) )

