{
  description = "A library and client implementation of luarocks";

  nixConfig = {
    extra-substituters = "https://neorocks.cachix.org";
    extra-trusted-public-keys = "neorocks.cachix.org-1:WqMESxmVTOJX7qoBC54TwrMMoVI1xAM+7yFin8NRfwk=";
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    git-hooks = {
      # TODO: https://github.com/cachix/git-hooks.nix/pull/396
      # url = "github:cachix/git-hooks.nix";
      url = "github:mrcjkb/git-hooks.nix?ref=clippy";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    self,
    nixpkgs,
    flake-parts,
    git-hooks,
    ...
  }: let
    overlay = import ./nix/overlay.nix {inherit self;};
  in
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      perSystem = {system, ...}: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            overlay
          ];
        };
        git-hooks-check = git-hooks.lib.${system}.run {
          src = self;
          hooks = {
            # NOTE: When adding/removing hooks, make sure
            # to update CONTRIBUTING.md for non-nix users.
            alejandra.enable = true;
            rustfmt.enable = true;
            clippy = {
              enable = true;
              settings = {
                denyWarnings = true;
                allFeatures = true;
              };
              extraPackages = pkgs.rocks.buildInputs ++ pkgs.rocks.nativeBuildInputs;
            };
            cargo-check.enable = true;
          };
          settings = {
            rust.check.cargoDeps = pkgs.rustPlatform.importCargoLock {
              lockFile = ./Cargo.lock;
            };
          };
        };
      in {
        packages = with pkgs; {
          default = rocks;
          inherit rocks;
        };

        devShells.default = pkgs.mkShell {
          name = "rocks devShell";
          inherit (git-hooks-check) shellHook;
          buildInputs =
            (with pkgs; [
              rust-analyzer
              cargo-nextest
            ])
            ++ self.checks.${system}.git-hooks-check.enabledPackages
            ++ pkgs.rocks.buildInputs
            ++ pkgs.rocks.nativeBuildInputs;
        };

        checks = rec {
          default = tests;
          inherit
            git-hooks-check
            ;
          tests = pkgs.rocks-debug;
        };
      };
      flake = {
        overlays.default = overlay;
      };
    };
}
