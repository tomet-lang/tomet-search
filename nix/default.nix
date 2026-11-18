{
  flake-parts,
  ...
}@inputs:
flake-parts.lib.mkFlake { inherit inputs; } {
  systems = [
    "x86_64-linux"
    "aarch64-linux"
    "aarch64-darwin"
  ];
  imports = [
    inputs.treefmt-nix.flakeModule
  ];

  perSystem =
    { pkgs, ... }:
    let
      system = pkgs.stdenv.hostPlatform.system;
      tometPkgs = inputs.tomet.packages.${system};
      twritPkgs = inputs.twrit.packages.${system};
      catastoPkgs = inputs.catasto.packages.${system};
      sishoPkgs = inputs.sisho.packages.${system};
    in
    {
      devShells.default = pkgs.callPackage ./dev.nix {
        inherit inputs;
        fenix = inputs.fenix.packages.${system};
        inherit (tometPkgs) tomet;
        inherit (twritPkgs) twrit;
        inherit (catastoPkgs) catasto;
        inherit (sishoPkgs) sisho;
      };

      treefmt = import ./formatter.nix {
        inherit (tometPkgs) tomet;
      };
    };
}
