# Launchpi

Open-source multi-functional midi controller platform for the novation launchpads.

## Tested Devices

- Launchpad Mini Mk1
- Launchpad Mini Mk3
- Streamdeck Studio
- Streamdeck Network Dock
- Streamdeck XL

## Instalation

### Nix

If you just want it up and running

```sh
nix run github:v3xlabs/launchpi#launchpi
```

The main way to run it is long-term is as a service:

```nix
{
  inputs.launchpi.url = "github:v3xlabs/launchpi";

  outputs = { nixpkgs, launchpi, ... }: {
    nixosConfigurations.host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        launchpi.nixosModules.default
      ];
    };
  };
}
```

```nix
services.launchpi = {
    enable = true;
    host = "127.0.0.1";
    port = 3000;
};
```

But you can also install it as a package:

```nix
{
  inputs.launchpi.url = "github:v3xlabs/launchpi";

  outputs = { nixpkgs, launchpi, ... }: {
    nixosConfigurations.host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        {
          environment.systemPackages = [
            launchpi.packages.x86_64-linux.launchpi
          ];
        }
      ];
    };
  };
}
```
