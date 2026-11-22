{
  description = "BARAS Parser for NixOS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      packages.${system} = {
        baras = pkgs.callPackage ./default.nix { };
        default = self.packages.${system}.baras;
      };

      apps.${system}.default = {
        type = "app";
        program = "${self.packages.${system}.baras}/bin/baras";
      };
    };
}
