{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    {
      self,
      nixpkgs,
      ...
    }:
    let
      inherit (nixpkgs) lib;

      systems = lib.systems.flakeExposed;

      forAllSystems = lib.genAttrs systems;

      nixpkgsFor = forAllSystems (
        system:
        import nixpkgs { inherit system; }
      );
    in
    {

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgsFor.${system};
        in
        {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              cargo
              openssl
              pkg-config
              rustc
            ];
          };
        }
      );

      overlays.default = final: prev: {
        humblebundle-games = final.callPackage ./build.nix { };
      };

      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgsFor.${system};
          overlay = lib.makeScope pkgs.newScope (final: self.overlays.default final pkgs);
        in
        {
          inherit (overlay) humblebundle-games;
          default = overlay.humblebundle-games;
        }
      );

      checks = forAllSystems (system: {
        package = self.packages.${system}.default;
      });
    };
}
