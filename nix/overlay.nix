{self}: final: prev: let
  # can't seem to override the buildType with override or overrideAttrs :(
  mk-rocks = {buildType ? "release"}:
    with final;
      rustPlatform.buildRustPackage {
        pname = "rocks";
        version = ((lib.importTOML "${self}/rocks-bin/Cargo.toml").package).version;

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
          installManPage target/dist/rocks.1
          installShellCompletion target/dist/rocks.{bash,fish} --zsh target/dist/_rocks
        '';

        env = {
          # disable vendored packages
          LIBGIT2_NO_VENDOR = 1;
          LIBSSH2_SYS_USE_PKG_CONFIG = 1;
          ROCKS_SKIP_IMPURE_TESTS = 1;
        };

        inherit buildType;

        meta.mainProgram = "rocks";
      };
in {
  rocks = mk-rocks {};
  rocks-debug = mk-rocks {buildType = "debug";};
}
