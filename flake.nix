{
  description = "ssh-portfolio development environment";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        rust = pkgs.rust-bin.nightly."2025-06-20".default.override {
          extensions = [ "clippy" "rust-analyzer" "cargo" "rustc" ];
        };

      in
      {
        devShell = pkgs.mkShell {
          buildInputs = [
            rust
            pkgs.cargo-deny
            pkgs.bun
            pkgs.nixpkgs-fmt
            pkgs.stdenv
            pkgs.git
            pkgs.zlib
            pkgs.docker
          ];

          shellHook = ''
            # Use host's default shell to make it more homely
            if [[ $- == *i* ]]; then
              export SHELL=$(getent passwd $USER | cut -d: -f7)
              exec $SHELL
            fi
          '';
        };
      });
}

