local git_ref = '6e883a2adea9414799300699e78c0d2f032b5c46'
local modrev = '0.0.43'
local specrev = '1'

local repo_url = 'https://github.com/tree-sitter/tree-sitter-rust'

rockspec_format = '3.0'
package = 'tree-sitter-rust'
version = modrev ..'-'.. specrev

description = {
  summary = 'tree-sitter parser for rust',
  labels = { 'neovim', 'tree-sitter' } ,
  homepage = 'https://github.com/tree-sitter/tree-sitter-rust',
  license = 'UNKNOWN'
}

dependencies = { 'lua >= 5.1' }

build_dependencies = {
  'luarocks-build-treesitter-parser >= 5.0.0',
}

source = {
  url = repo_url .. '/archive/' .. git_ref .. '.zip',
  dir = 'tree-sitter-rust-' .. '6e883a2adea9414799300699e78c0d2f032b5c46',
}

build = {
  type = "treesitter-parser",
  lang = "rust",
  parser = true,
  generate = false,
  generate_from_json = false,
  location = nil,
  copy_directories = { "queries" },
  queries = {
    ["folds.scm"] = [==[
[
  (mod_item)
  (foreign_mod_item)
  (function_item)
  (struct_item)
  (trait_item)
  (enum_item)
  (impl_item)
  (type_item)
  (union_item)
  (const_item)
  (let_declaration)
  (loop_expression)
  (for_expression)
  (while_expression)
  (if_expression)
  (match_expression)
  (call_expression)
  (array_expression)
  (macro_definition)
  (macro_invocation)
  (attribute_item)
  (block)
  (use_declaration)+
] @fold
]==],
  },
}
