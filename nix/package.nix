{
  alsa-lib,
  rustPlatform,
  just,
  lib,
  nodejs,
  pkg-config,
  pnpm,
  pnpmConfigHook,
  fetchPnpmDeps,
  jack2,
}:
rustPlatform.buildRustPackage {
  pname = "launchpi";
  version = "0.0.1";

  src = lib.cleanSource ../.;

  cargoRoot = "daemon";
  cargoLock = {
    lockFile = ../daemon/Cargo.lock;
    outputHashes = {
      "launchy-0.3.0" = "sha256-DdqTCZiyOckjdiPeh2mu8FyCIsHv5sNigJkcAaZCB8Y=";
    };
  };

  pnpmRoot = "web";
  pnpmDeps = fetchPnpmDeps {
    pname = "launchpi-web";
    version = "0.0.1";
    src = ../web;
    fetcherVersion = 4;
    hash = "sha256-oOQ9Vn3Ap/NbpNlU1jbWUyDMODpj1b0+zmVetGWZwmk=";
  };

  nativeBuildInputs = [
    just
    nodejs
    pkg-config
    pnpm
    pnpmConfigHook
  ];

  buildInputs = [
    alsa-lib
    jack2
  ];

  buildPhase = ''
    runHook preBuild
    just build
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    install -Dm755 daemon/target/release/launchpi $out/bin/launchpi
    runHook postInstall
  '';

  meta = {
    description = "Multi-functional MIDI controller platform for Novation Launchpads";
    homepage = "https://github.com/v3xlabs/launchpi";
    mainProgram = "launchpi";
    platforms = lib.platforms.linux;
  };
}
