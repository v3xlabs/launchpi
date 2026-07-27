{
  description = "Open-source multi-functional MIDI controller platform for Novation Launchpads";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    rust-overlay,
  }:
    flake-utils.lib.eachSystem [
      "aarch64-linux"
      "x86_64-linux"
    ] (system: let
      pkgs = import nixpkgs {
        inherit system;
      };

      launchpi = pkgs.callPackage ./nix/package.nix {};
    in {
      packages = {
        inherit launchpi;
        default = launchpi;
      };

      apps = {
        launchpi = {
          type = "app";
          program = "${launchpi}/bin/launchpi";
        };
        default = {
          type = "app";
          program = "${launchpi}/bin/launchpi";
        };
      };

      devShells.default = import ./nix/devshell.nix {
        inherit nixpkgs rust-overlay system;
      };
    })
    // {
      nixosModules.default = import ./nix/module.nix {inherit self;};
    };
}
