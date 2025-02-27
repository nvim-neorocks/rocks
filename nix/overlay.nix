{
  self,
  crane,
}: final: prev: let
  cleanCargoSrc = craneLib.cleanCargoSource self;

  craneLib = crane.mkLib prev;

  commonArgs = with final; {
    inherit (craneLib.crateNameFromCargoToml {cargoToml = "${self}/lux-cli/Cargo.toml";}) version pname;
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

  # can't seem to override the buildType with override or overrideAttrs :(
  mk-lux-cli = {buildType ? "release"}:
    craneLib.buildPackage (individualCrateArgs
      // {
        pname = "lux-cli";
        cargoExtrArgs = "-p lux-cli";

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
in {
  inherit lux-deps;
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
