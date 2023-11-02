{
  pkgs,
  makeRustPlatform,
  nightly-rust,
  lib,
}: let
  platform = makeRustPlatform {
    cargo = nightly-rust;
    rustc = nightly-rust;
  };
in
  platform.buildRustPackage {
    name = "humblebundle-games";
    cargoLock.lockFile = ./Cargo.lock;

    src = ./.;

    nativeBuildInputs = with pkgs; [
      pkg-config
    ];

    buildInputs = with pkgs; [
      openssl
    ] ++ lib.optionals stdenv.isDarwin [
      pkgs.darwin.apple_sdk.frameworks.Foundation
    ];
  }
