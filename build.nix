{
  cmake,
  darwin,
  installShellFiles,
  lib,
  openssl,
  pkg-config,
  rustPlatform,
  stdenv,
}:
rustPlatform.buildRustPackage {
  pname = "humblebundle-games";
  version = "0.3.0";

  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [
    cmake
    installShellFiles
    pkg-config
  ];

  buildInputs = [
    openssl
  ]
  ++ lib.optionals stdenv.isDarwin [
    darwin.apple_sdk.frameworks.Foundation
  ];

  postInstall = lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
    installShellCompletion --cmd humblebundle-games \
      --bash <($out/bin/humblebundle-games --completions bash) \
      --fish <($out/bin/humblebundle-games --completions fish) \
      --zsh  <($out/bin/humblebundle-games --completions zsh)
  '';

  meta = {
    mainProgram = "humblebundle-games";
  };
}
