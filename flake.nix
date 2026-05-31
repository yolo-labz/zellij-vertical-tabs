{
  description = "zellij-vertical-tabs — a robust vertical tab-bar sidebar plugin for zellij (Catppuccin Mocha)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    fenix,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      # nixpkgs rustc ships no wasm32-wasip1 std; fenix supplies the target.
      toolchain = fenix.packages.${system}.combine [
        fenix.packages.${system}.stable.rustc
        fenix.packages.${system}.stable.cargo
        fenix.packages.${system}.targets.wasm32-wasip1.stable.rust-std
      ];
      rustPlatform = pkgs.makeRustPlatform {
        cargo = toolchain;
        rustc = toolchain;
      };
    in {
      # `nix build` -> result/lib/zellij-vertical-tabs.wasm
      #
      # Built WASM-ONLY on purpose: zellij-tile pulls zellij-utils (curl/openssl
      # etc.) which only compile for the host. We never compile the host target,
      # so those host-only crates are vendored-but-not-built. cargoSetupHook
      # vendors deps from Cargo.lock and writes an offline cargo config.
      packages.default = pkgs.stdenv.mkDerivation {
        pname = "zellij-vertical-tabs";
        version = "0.1.0";
        src = ./.;
        cargoDeps = rustPlatform.importCargoLock {lockFile = ./Cargo.lock;};
        nativeBuildInputs = [toolchain rustPlatform.cargoSetupHook];
        buildPhase = ''
          runHook preBuild
          cargo build --release --target wasm32-wasip1 --offline
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          mkdir -p $out/lib
          cp target/wasm32-wasip1/release/zellij-vertical-tabs.wasm $out/lib/
          runHook postInstall
        '';
        meta = {
          description = "Robust vertical tab-bar sidebar plugin for zellij";
          license = pkgs.lib.licenses.mit;
          platforms = pkgs.lib.platforms.all;
        };
      };

      devShells.default = pkgs.mkShell {
        packages = [toolchain pkgs.zellij pkgs.wabt];
        shellHook = ''
          echo "zellij-vertical-tabs dev shell"
          echo "  cargo build --release --target wasm32-wasip1"
        '';
      };

      formatter = pkgs.alejandra;
    });
}
