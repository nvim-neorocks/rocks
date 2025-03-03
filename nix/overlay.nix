{
  self,
  crane,
}: final: prev: let
  cleanCargoSrc = craneLib.cleanCargoSource self;

  craneLib = crane.mkLib prev;

  commonArgs = with final; {
    strictDeps = true;

    nativeBuildInputs = [
      pkg-config
      installShellFiles
    ];

    buildInputs =
      [
        luajit
        openssl
        libgit2
        gnupg
        libgpg-error
        gpgme
      ]
      ++ lib.optionals stdenv.isDarwin [
        darwin.apple_sdk.frameworks.Security
        darwin.apple_sdk.frameworks.SystemConfiguration
      ];

    env = {
      # disable vendored packages
      LIBGIT2_NO_VENDOR = 1;
      LIBSSH2_SYS_USE_PKG_CONFIG = 1;
      LUX_SKIP_IMPURE_TESTS = 1;
    };
  };

  lux-deps = craneLib.buildDepsOnly (commonArgs
    // {
      pname = "lux";
      version = "0.1.0";
      src = cleanCargoSrc;
    });

  individualCrateArgs =
    commonArgs
    // {
      src = cleanCargoSrc;
      cargoArtifacts = lux-deps;
      # NOTE: We disable tests since we run them via cargo-nextest in a separate derivation
      doCheck = false;
    };

  fileSetForCrate = with final; crate: lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      /./${crate}/Cargo.toml
      # (craneLib.fileset.commonCargoSources ../lux-hack)
      (craneLib.fileset.commonCargoSources crate)
    ];
  };

  mk-lux-lib = {buildType ? "release"}:
    craneLib.buildPackage (individualCrateArgs
      // {
        pname = "lux-lib";
        inherit (craneLib.crateNameFromCargoToml { src = ../lux-lib; }) version;
        src = fileSetForCrate ../lux-lib;
        cargoExtraArgs = "-p lux-lib";

        inherit buildType;
      });

  # can't seem to override the buildType with override or overrideAttrs :(
  mk-lux-cli = {buildType ? "release"}:
    craneLib.buildPackage (individualCrateArgs
      // {
        pname = "lux-cli";
        inherit (craneLib.crateNameFromCargoToml { src = ../lux-cli; }) version;
        src = fileSetForCrate ../lux-cli;
        cargoExtraArgs = "-p lux-cli";

        postBuild = ''
          cargo xtask dist-man
          cargo xtask dist-completions
        '';

        postInstall = ''
          installManPage target/dist/lux.1
          installShellCompletion target/dist/lux.{bash,fish} --zsh target/dist/_lux
        '';

        inherit buildType;

        meta.mainProgram = "lux";
      });

   # Ensure that cargo-hakari is up to date
   # lux-hakari = craneLib.mkCargoDerivation {
   #   src = self;
   #   pname = "lux-hakari";
   #   cargoArtifacts = null;
   #   doInstallCargoArtifacts = false;
   #
   #   buildPhaseCargoCommand = ''
   #     cargo hakari generate --diff  # workspace-hack Cargo.toml is up-to-date
   #     cargo hakari manage-deps --dry-run  # all workspace crates depend on workspace-hack
   #     cargo hakari verify
   #   '';
   #
   #   nativeBuildInputs = with final; [
   #     cargo-hakari
   #   ];
   # };
in {
  inherit lux-deps; # lux-hakari;
  lux-lib = mk-lux-lib {};
  lux-lib-debug = mk-lux-lib {buildType = "debug";};

  lux-cli = mk-lux-cli {};
  lux-cli-debug = mk-lux-cli {buildType = "debug";};

  lux-nextest = craneLib.cargoNextest (commonArgs
    // {
      src = self;
      nativeCheckInputs = with final; [
        cacert
        cargo-nextest
        zlib # used for checking external dependencies
        lua
        nix # we use nix-hash in tests
      ];

      preCheck = ''
        export HOME=$(realpath .)
      '';

      cargoArtifacts = lux-deps;
      partitions = 1;
      partitionType = "count";
      cargoNextestExtraArgs = "--no-fail-fast --lib"; # Disable integration tests
      cargoNextestPartitionsExtraArgs = "--no-tests=pass";
    });

  lux-clippy = craneLib.cargoClippy (commonArgs
    // {
      src = self;
      cargoArtifacts = lux-deps;
    });
}
