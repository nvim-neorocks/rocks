{
  description = "Luarocks <3 Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    pre-commit-hooks = {
      # TODO: https://github.com/cachix/pre-commit-hooks.nix/pull/396
      # url = "github:cachix/pre-commit-hooks.nix";
      url = "github:mrcjkb/pre-commit-hooks.nix/clippy";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    self,
    nixpkgs,
    flake-parts,
    pre-commit-hooks,
    ...
  }: let
    overlay = import ./nix/overlay.nix {inherit self;};
  in
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = [
        "x86_64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      perSystem = {
        config,
        self',
        inputs',
        pkgs,
        system,
        ...
      }: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            overlay
          ];
        };
        pre-commit-check = pre-commit-hooks.lib.${system}.run {
          src = self;
          hooks = {
            alejandra.enable = true;
            rustfmt.enable = true;
            clippy = {
              enable = true;
              settings = {
                denyWarnings = true;
                allFeatures = true;
              };
            };
            cargo-check.enable = true;
          };
          settings = {
            runtimeDeps = pkgs.rocks.buildInputs ++ pkgs.rocks.nativeBuildInputs;
            rust.cargoDeps = pkgs.rustPlatform.importCargoLock {
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
          inherit (pre-commit-check) shellHook;
          buildInputs =
            (with pkgs; [
              rust-analyzer
              cargo-nextest
            ])
            ++ (with pre-commit-hooks.packages.${system}; [
              alejandra
              rustfmt
              clippy
            ])
            ++ pkgs.rocks.buildInputs
            ++ pkgs.rocks.nativeBuildInputs;
        };

        checks = with pkgs; {
          inherit
            pre-commit-check
            rocks
            ;
        };
      };
      flake = {
        overlays.default = overlay;
      };
    };
}
