{
  description = "ssh-portfolio development environment";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";
    bun2nix.url = "github:nix-community/bun2nix";

    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    bun2nix.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      crane,
      rust-overlay,
      bun2nix,
      ...
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            (import rust-overlay)
            bun2nix.overlays.default
          ];
        };

        # --- Libraries ---
        lib = pkgs.lib;
        craneLib = (crane.mkLib pkgs).overrideToolchain (
          toolchain:
          toolchain.rust-bin.nightly."2025-06-20".default.override {
            extensions = [
              "clippy"
              "rust-analyzer"
              "cargo"
              "rustc"
            ];
          }
        );

        # Build www project
        www = pkgs.stdenv.mkDerivation {
          name = "ssh-portfolio-www";
          packageJson = ./www/package.json;
          src = ./www;

          nativeBuildInputs = [ pkgs.bun2nix.hook ];
          bunDeps = pkgs.bun2nix.fetchBunDeps { bunNix = ./www/bun.nix; };

          buildPhase = ''
            bun run build
          '';
          installPhase = ''
            cp -r build/ $out
          '';
          checkPhase = ''
            bun run check
            bun run lint
          '';
        };

        # Base arguments passed to almost all crane invocations
        commonCraneArgs = {
          # Include source code and other files required to build, exclude `.cargo` containing `cargo-rustc-patch`
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions ([
              (lib.fileset.difference (craneLib.fileset.commonCargoSources ./.) (lib.fileset.fromSource ./.cargo))
              (lib.fileset.fromSource ./.config)
              (lib.fileset.fromSource ./assets)
              (lib.fileset.fromSource ./patches)
              (lib.fileset.fromSource ./src/atproto/lexicons)
            ]);
          };
          strictDeps = true;
          nativeBuildInputs = with pkgs; [
            pkg-config
            zlib
            bun
          ];
        };

        # Apply patches located in the `./patches/` dir
        patches = lib.fileset.fromSource ./patches;
        cargoVendorDir = builtins.toString (
          craneLib.vendorCargoDeps (
            commonCraneArgs
            // {
              overrideVendorCargoPackage =
                p: drv:
                let
                  attrs = (lib.groupBy (p: builtins.baseNameOf (builtins.dirOf p)) (lib.fileset.toList patches));
                  key = "${p.name}-${p.version}";
                in
                if (builtins.hasAttr key attrs) then
                  builtins.toString (drv.overrideAttrs { patches = attrs.${key}; })
                else
                  drv;
            }
          )
        );

        # Build dependencies separately to have them cached in nix store
        cargoArtifacts = craneLib.buildDepsOnly (
          commonCraneArgs
          // {
            inherit cargoVendorDir;
          }
        );

        # Finally, compile the actual project
        crate =
          {
            features ? [ ],
          }:
          craneLib.buildPackage (
            commonCraneArgs
            // {
              inherit cargoArtifacts cargoVendorDir;
              doChecks = false;
              cargoExtraArgs = "--locked --no-default-features ${
                lib.optionalString (features != [ ]) ("--features " + lib.concatStringsSep "," features)
              }";
              preBuild = ''
                mkdir -p www/build
                cp -r ${www} www/build
              '';
            }
          );

        ssh-portfolio = pkgs.callPackage crate { };
        ssh-portfolio-deny = craneLib.cargoDeny (
          commonCraneArgs
          // {
            cargoDenyChecks = "--hide-inclusion-graph bans licenses sources";
          }
        );

      in
      {
        apps.default = flake-utils.lib.mkApp { drv = ssh-portfolio; };
        packages = {
          default = ssh-portfolio;
          inherit ssh-portfolio;
        };

        checks = {
          inherit ssh-portfolio ssh-portfolio-deny www;
          formatting = pkgs.runCommandLocal "treefmt-check" { buildInputs = [ pkgs.nixfmt-tree ]; } ''
            set -euo pipefail
            cp -r ${./.} workdir
            chmod -R +w workdir/
            treefmt --ci --tree-root workdir/ --excludes '**/bun.nix'
            touch $out
          '';
        };

        formatter = pkgs.nixfmt-tree;
        devShells.default = craneLib.devShell {
          name = "ssh-portfolio";
          inputsFrom = [ ssh-portfolio ];
          checks = self.checks.${system};
          packages = with pkgs; [
            bun
            bun2nix.packages.${system}.default
            git
            nixfmt-tree
            docker
          ];

          shellHook = ''
            # Use host's default shell to make it more homely
            if [[ $- == *i* ]]; then
              export SHELL=$(getent passwd $USER | cut -d: -f7)
              exec $SHELL
            fi
          '';
        };
      }
    );
}
