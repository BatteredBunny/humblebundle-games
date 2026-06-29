{
  lib,
  openssl,
  pkg-config,
  rustPlatform,
  stdenv,
  darwin,
}:
rustPlatform.buildRustPackage {
  pname = "humblebundle-games";
  version = "0.3.0";

  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [
    pkg-config
  ];

  buildInputs = [
    openssl
  ]
  ++ lib.optionals stdenv.isDarwin [
    darwin.apple_sdk.frameworks.Foundation
  ];

  meta = {
    mainProgram = "humblebundle-games";
  };
}
