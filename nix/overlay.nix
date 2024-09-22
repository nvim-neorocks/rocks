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
        ];

        buildInputs =
          [
            luajit
            openssl
            libgit2
          ]
          ++ lib.optionals stdenv.isDarwin [
            darwin.apple_sdk.frameworks.Security
            darwin.apple_sdk.frameworks.SystemConfiguration
          ];

        nativeCheckInputs = [
          cacert
          cargo-nextest
        ];

        preCheck = ''
          export HOME=$(realpath .)
        '';

        checkPhase = ''
          runHook preCheck
          # Disable integration tests
          cargo nextest run --no-fail-fast --lib
          runHook postCheck
        '';

        env = {
          # disable vendored packages
          LIBGIT2_NO_VENDOR = 1;
          LIBSSH2_SYS_USE_PKG_CONFIG = 1;
        };

        inherit buildType;
      };
in {
  rocks = mk-rocks {};
  rocks-debug = mk-rocks {buildType = "debug";};
}
