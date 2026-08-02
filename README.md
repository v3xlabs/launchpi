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

The service runs as the `launchpi` system user by default. Set `user`, `group`,
and `extraGroups` when it needs access through an existing account or host
hardware groups. `openFirewall` opens the configured HTTP port. Set
`discovery = false` when every Stream Deck is listed in the configuration.

```nix
services.launchpi = {
  enable = true;
  user = "launchpi";
  extraGroups = [ "audio" ];
  host = "0.0.0.0";
  openFirewall = true;
  discovery = false;
};
```

### Declarative configuration

`services.launchpi.settings` generates an immutable configuration directory.
Changes made in the web UI remain active until Launchpi restarts, then the
declarative configuration is loaded again. Export temporary changes from the UI
and add them to `settings` to keep them.

```nix
services.launchpi = {
  enable = true;
  discovery = false;

  settings = {
    devices = {
      version = 1;
      devices = [
        {
          surface_id = "studio";
          name = "Studio";
          host = "10.0.0.195";
          port = 5343;
          model = "Stream Deck Studio";
          layout = {
            Grid = {
              columns = 8;
              rows = 4;
            };
          };
          capabilities = {
            supports_color = true;
            supports_images = true;
            supports_text = true;
            supports_brightness = true;
            supports_haptics = false;
          };
          active_panel_id = "main";
          is_enabled = true;
        }
      ];
    };

    panels = {
      version = 5;
      panels = [
        {
          panel_id = "main";
          name = "Main";
          layout = {
            columns = 8;
            rows = 4;
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
    };

    plugins."mpris.default" = {
      version = 1;
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
