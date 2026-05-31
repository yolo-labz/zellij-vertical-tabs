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
      # nixpkgs rustc ships no wasm32-wasip1 std; fenix provides the target.
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
      packages.default = rustPlatform.buildRustPackage {
        pname = "zellij-vertical-tabs";
        version = "0.1.0";
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;
        # build the wasm guest, not a host binary
        CARGO_BUILD_TARGET = "wasm32-wasip1";
        doCheck = false;
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

      # expose the wasm path directly for downstream layouts (e.g. the NixOS flake)
      packages.wasm = self.packages.${system}.default;

      devShells.default = pkgs.mkShell {
        packages = [toolchain pkgs.zellij pkgs.wabt];
        shellHook = ''
          echo "zellij-vertical-tabs dev shell"
          echo "  cargo build --release --target wasm32-wasip1"
          echo "  zellij --layout examples/right.kdl   # smoke test"
        '';
      };

      formatter = pkgs.alejandra;
    });
}
