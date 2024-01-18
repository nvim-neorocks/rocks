{self}: final: prev: {
  rocks = with final;
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

      buildInputs = [
        luajit
        openssl
      ];
    };
}
