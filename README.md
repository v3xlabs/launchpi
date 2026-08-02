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

If you just want it up and running to try it out

```sh
nix run github:v3xlabs/launchpi#launchpi
```

Tho most likely you will want to run it as a service:

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

### Declarative configuration

`services.launchpi.settings` generates an immutable configuration directory.
Changes made in the web UI remain active until Launchpi restarts, then the
declarative configuration is loaded again. Export temporary changes from the UI
and add them to `settings` to keep them.

Known Stream Deck models provide their own layout and capabilities. Set only
`model`; do not configure device layout or capabilities.

The copy-TOML buttons export complete panel and device documents. Add those
files directly to the matching list; their document versions are ignored and
the module writes the current version.

```nix
services.launchpi.settings = {
  devices = [
    ./devices/network-dock.toml
    ./devices/studio.toml
  ];

  panels = [
    ./panels/main.toml
    ./panels/media.toml
    ./panels/lights.toml
    ./panels/weather.toml
  ];
};
```

```nix
services.launchpi = {
  enable = true;
  discovery = false;

  settings = {
    devices = [
      {
        surface_id = "studio";
        name = "Studio";
        host = "10.0.0.195";
        port = 5343;
        model = "Stream Deck Studio";
        active_panel_id = "main";
        enable = true;
      }
    ];

    panels = [
      {
        panel_id = "main";
        name = "Main";
        layout = {
          columns = 16;
          rows = 2;
        };
        capabilities = {
          supports_color = true;
          supports_images = true;
          supports_text = true;
          supports_brightness = true;
          supports_haptics = false;
        };
        controls = [
          {
            control_id = "welcome";
            name = "Welcome";
            position = {
              column = 0;
              row = 0;
            };
            default_state = {
              is_pressed = false;
              layers = [
                {
                  kind = "fill";
                  color = {
                    red = 30;
                    green = 41;
                    blue = 59;
                    alpha = 255;
                  };
                }
                {
                  kind = "text";
                  text = "Hello";
                  color = {
                    red = 255;
                    green = 255;
                    blue = 255;
                    alpha = 255;
                  };
                }
              ];
            };
            action_bindings = [];
          }
        ];
      }
    ];

    plugins."mpris.default" = {
      enabled = true;
      display_name = "Local media";
      config.preferred_player = "spotify";
    };
  };
};
```

Use `configDir` instead of `settings` for a writable configuration directory.
`stateDir` stores runtime state and `cacheDir` stores downloaded and decoded
assets.

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
