{self}: final: prev: let
  # can't seem to override the buildType with override or overrideAttrs :(
  mk-lux = {buildType ? "release"}:
    with final;
      rustPlatform.buildRustPackage {
        pname = "lux";
        version = ((lib.importTOML "${self}/lux-cli/Cargo.toml").package).version;

        src = self;

        cargoLock = {
          lockFile = ../Cargo.lock;
        };

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

        nativeCheckInputs = [
          cacert
          cargo-nextest
          zlib # used for checking external dependencies
          lua
          nix # we use nix-hash in tests
        ];

        postBuild = ''
          cargo xtask dist-man
          cargo xtask dist-completions
        '';

        preCheck = ''
          export HOME=$(realpath .)
        '';

        checkPhase = ''
          runHook preCheck
          # Disable integration tests
          cargo nextest run --no-fail-fast --lib
          runHook postCheck
        '';

        postInstall = ''
          installManPage target/dist/lux.1
          installShellCompletion target/dist/lux.{bash,fish} --zsh target/dist/_lux
        '';

        env = {
          # disable vendored packages
          LIBGIT2_NO_VENDOR = 1;
          LIBSSH2_SYS_USE_PKG_CONFIG = 1;
          LUX_SKIP_IMPURE_TESTS = 1;
        };

        inherit buildType;

        meta.mainProgram = "lux";
      };
in {
  lux = mk-lux {};
  lux-debug = mk-lux {buildType = "debug";};
}
