{
  description = "Rust dev shell template";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      rec {
        devShells.default =
          with pkgs;
          mkShell {
            packages = [
              clippy
              rustfmt
              rust-analyzer
            ];
            nativeBuildInputs = [
              cargo
              pkg-config
              rustc
            ];
            shellHook = ''
              # handle space in path
              export NIX_LDFLAGS="''${NIX_LDFLAGS/-rpath $out\/lib /}"
            '';
          };

        packages.diskonaut = pkgs.rustPlatform.buildRustPackage {
          pname = "diskonaut";
          version = "0.11.0";

          src = ./.;

          cargoHash = "sha256-F5/3ekwua5GaCW8vRU0HU708ThXRWleeah0Nrzlbnf8=";
        };

        packages.default = packages.diskonaut;
      }
    );
}
