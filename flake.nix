{
  description = "A library and client implementation of luarocks";

  nixConfig = {
    extra-substituters = "https://neorocks.cachix.org";
    extra-trusted-public-keys = "neorocks.cachix.org-1:WqMESxmVTOJX7qoBC54TwrMMoVI1xAM+7yFin8NRfwk=";
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-parts.url = "github:hercules-ci/flake-parts";
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    self,
    nixpkgs,
    flake-parts,
    git-hooks,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = builtins.attrNames nixpkgs.legacyPackages;
      perSystem = attrs @ {
        system,
        pkgs,
        ...
      }: let
        pkgs = attrs.pkgs.extend self.overlays.default;
        git-hooks-check = git-hooks.lib.${system}.run {
          src = self;
          hooks = {
            # NOTE: When adding/removing hooks, make sure
            # to update CONTRIBUTING.md for non-nix users.
            alejandra.enable = true;
            rustfmt.enable = true;
            taplo.enable = true;
          };
        };
      in {
        packages = with pkgs; {
          default = lux;
          inherit lux;
          inherit lux-lib;
        };

        devShells = let
          mkDevShell = lua_pkg:
            pkgs.mkShell {
              name = "lux devShell";
              inherit (git-hooks-check) shellHook;
              buildInputs =
                (with pkgs; [
                  rust-analyzer
                  ra-multiplex
                  cargo-nextest
                  clippy
                  lua_pkg
                  # Needed for integration test builds
                  pkg-config
                  libxcrypt
                  cmakeMinimal
                  zlib
                ])
                ++ self.checks.${system}.git-hooks-check.enabledPackages
                ++ pkgs.lux-cli.buildInputs
                ++ pkgs.lux-cli.nativeBuildInputs;
            };
        in rec {
          default = lua51;
          lua51 = mkDevShell pkgs.lua5_1;
          lua52 = mkDevShell pkgs.lua5_2;
          lua53 = mkDevShell pkgs.lua5_3;
          lua54 = mkDevShell pkgs.lua5_4;
          luajit = mkDevShell pkgs.luajit;
        };

        checks = rec {
          default = tests;
          inherit
            git-hooks-check
            ;
          tests = pkgs.lux-nextest;
          clippy = pkgs.lux-clippy;
        };
      };
      flake = {
        overlays.default = with inputs; import ./nix/overlay.nix {inherit self crane;};
      };
    };
}
