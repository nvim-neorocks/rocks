{
  description = "Luarocks <3 Rust";

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
        git-check = git-hooks.lib.${system}.run {
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
          inherit (git-check) shellHook;
          buildInputs =
            (with pkgs; [
              rust-analyzer
              cargo-nextest
            ])
            ++ (with git-hooks.packages.${system}; [
              alejandra
              rustfmt
              clippy
            ])
            ++ pkgs.rocks.buildInputs
            ++ pkgs.rocks.nativeBuildInputs;
        };

        checks = with pkgs; {
          inherit
            git-check
            rocks
            ;
        };
      };
      flake = {
        overlays.default = overlay;
      };
    };
}
