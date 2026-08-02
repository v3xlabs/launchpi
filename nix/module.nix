{self}: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.launchpi;
  inherit (lib) types;

  rgbaColor = types.submodule {
    options = {
      red = lib.mkOption {type = types.ints.between 0 255;};
      green = lib.mkOption {type = types.ints.between 0 255;};
      blue = lib.mkOption {type = types.ints.between 0 255;};
      alpha = lib.mkOption {type = types.ints.between 0 255;};
    };
  };

  color = types.either types.str rgbaColor;

  capabilities = types.submodule {
    options = {
      supports_color = lib.mkOption {type = types.bool; default = false;};
      supports_images = lib.mkOption {type = types.bool; default = false;};
      supports_text = lib.mkOption {type = types.bool; default = false;};
      supports_brightness = lib.mkOption {type = types.bool; default = false;};
      supports_haptics = lib.mkOption {type = types.bool; default = false;};
    };
  };

  layer = types.submodule {
    options = {
      kind = lib.mkOption {
        type = types.enum ["fill" "image" "text" "bar" "border"];
      };
      color = lib.mkOption {type = types.nullOr color; default = null;};
      text = lib.mkOption {type = types.nullOr types.str; default = null;};
      image = lib.mkOption {type = types.nullOr types.str; default = null;};
      fit = lib.mkOption {
        type = types.nullOr (types.enum ["cover" "contain"]);
        default = null;
      };
      anchor = lib.mkOption {
        type = types.nullOr (types.enum [
          "top_start"
          "top_center"
          "top_end"
          "center_start"
          "center"
          "center_end"
          "bottom_start"
          "bottom_center"
          "bottom_end"
        ]);
        default = null;
      };
      scale_percent = lib.mkOption {
        type = types.nullOr (types.ints.between 0 100);
        default = null;
      };
      tint = lib.mkOption {type = types.nullOr color; default = null;};
      font_family = lib.mkOption {type = types.nullOr types.str; default = null;};
      font_size = lib.mkOption {
        type = types.nullOr (types.ints.between 1 255);
        default = null;
      };
      value = lib.mkOption {type = types.nullOr types.anything; default = null;};
      maximum = lib.mkOption {type = types.nullOr types.anything; default = null;};
      edge = lib.mkOption {
        type = types.nullOr (types.enum ["top" "bottom" "start" "end"]);
        default = null;
      };
      thickness = lib.mkOption {
        type = types.nullOr (types.ints.between 1 255);
        default = null;
      };
      width = lib.mkOption {
        type = types.nullOr (types.ints.between 1 255);
        default = null;
      };
    };
  };

  renderedState = types.submodule {
    options = {
      is_pressed = lib.mkOption {type = types.bool;};
      layers = lib.mkOption {type = types.listOf layer; default = [];};
    };
  };

  control = types.submodule {
    options = {
      control_id = lib.mkOption {type = types.str;};
      name = lib.mkOption {type = types.str;};
      position = lib.mkOption {
        type = types.submodule {
          options = {
            column = lib.mkOption {type = types.ints.between 0 65535;};
            row = lib.mkOption {type = types.ints.between 0 65535;};
          };
        };
      };
      default_state = lib.mkOption {type = renderedState;};
      pressed_state = lib.mkOption {type = types.nullOr renderedState; default = null;};
      action_bindings = lib.mkOption {
        type = types.listOf (types.attrsOf types.anything);
        default = [];
      };
    };
  };

  panel = types.submodule {
    options = {
      panel_id = lib.mkOption {type = types.str;};
      name = lib.mkOption {type = types.str;};
      layout = lib.mkOption {
        type = types.submodule {
          options = {
            columns = lib.mkOption {type = types.ints.between 0 65535;};
            rows = lib.mkOption {type = types.ints.between 0 65535;};
          };
        };
      };
      font_family = lib.mkOption {type = types.nullOr types.str; default = null;};
      capabilities = lib.mkOption {type = capabilities; default = {};};
      controls = lib.mkOption {type = types.listOf control; default = [];};
      dials = lib.mkOption {
        type = types.listOf (types.attrsOf types.anything);
        default = [];
      };
    };
  };

  device = types.submodule {
    options = {
      surface_id = lib.mkOption {type = types.str;};
      name = lib.mkOption {type = types.str;};
      host = lib.mkOption {type = types.str;};
      port = lib.mkOption {type = types.port; default = 5343;};
      serial_number = lib.mkOption {type = types.nullOr types.str; default = null;};
      model = lib.mkOption {
        type = types.enum [
          "Stream Deck"
          "Stream Deck Mini"
          "Stream Deck XL"
          "Stream Deck Mk.2"
          "Stream Deck Plus"
          "Stream Deck Neo"
          "Stream Deck Studio"
          "Stream Deck Network Dock"
        ];
      };
      active_panel_id = lib.mkOption {type = types.nullOr types.str; default = null;};
      enable = lib.mkOption {type = types.bool; default = true;};
    };
  };

  userValue = types.submodule {
    options = {
      name = lib.mkOption {type = types.str;};
      value = lib.mkOption {type = types.anything;};
      description = lib.mkOption {type = types.nullOr types.str; default = null;};
    };
  };

  plugin = types.submodule {
    options = {
      enabled = lib.mkOption {type = types.bool; default = true;};
      display_name = lib.mkOption {type = types.nullOr types.str; default = null;};
      config = lib.mkOption {type = types.attrsOf types.anything; default = {};};
    };
  };

  deviceSource = types.either types.path device;
  panelSource = types.either types.path panel;

  removeNulls = value:
    if builtins.isNull value
    then null
    else if builtins.isAttrs value
    then lib.filterAttrs (_: item: !builtins.isNull item) (lib.mapAttrs (_: removeNulls) value)
    else if builtins.isList value
    then map removeNulls value
    else value;

  documentEntries = field: path:
    let
      document = builtins.fromTOML (builtins.readFile path);
    in
      if builtins.hasAttr field document
      then document.${field}
      else throw "${toString path} does not define ${field}";

  configuredDevices = builtins.concatMap (
    source:
      if builtins.isPath source
      then documentEntries "devices" source
      else [removeNulls ((builtins.removeAttrs source ["enable"]) // {is_enabled = source.enable;})]
  ) cfg.settings.devices;

  configuredPanels = builtins.concatMap (
    source:
      if builtins.isPath source
      then documentEntries "panels" source
      else [removeNulls source]
  ) cfg.settings.panels;

  toml = pkgs.formats.toml {};
  settingsDir = pkgs.runCommand "launchpi-config" {} ''
    mkdir -p "$out/plugins"
    ln -s ${toml.generate "devices.toml" {
      version = 1;
      devices = configuredDevices;
    }} "$out/devices.toml"
    ln -s ${toml.generate "panels.toml" {version = 5; panels = configuredPanels;}} "$out/panels.toml"
    ln -s ${toml.generate "values.toml" {version = 1; values = removeNulls cfg.settings.values;}} "$out/values.toml"
    ${lib.concatStringsSep "\n" (lib.mapAttrsToList (name: value: ''
        ln -s ${toml.generate "${name}.toml" ({version = 1;} // removeNulls value)} "$out/plugins/${name}.toml"
      '')
      cfg.settings.plugins)}
  '';
  configDir =
    if cfg.settings == null
    then cfg.configDir
    else settingsDir;
in {
  options.services.launchpi = {
    enable = lib.mkEnableOption "Launchpi";

    package = lib.mkPackageOption self.packages.${pkgs.stdenv.hostPlatform.system} "launchpi" {};

    user = lib.mkOption {
      type = lib.types.str;
      default = "launchpi";
      description = "User account under which Launchpi runs.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "launchpi";
      description = "Primary group under which Launchpi runs.";
    };

    extraGroups = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [];
      description = "Supplementary groups for hardware access.";
    };

    host = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = "Address on which Launchpi serves its web interface and API.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 3000;
      description = "Port on which Launchpi serves its web interface and API.";
    };

    discovery = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Whether to discover devices through mDNS.";
    };

    openFirewall = lib.mkEnableOption "opening the Launchpi port in the firewall";

    configDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/launchpi/config";
      description = "Directory containing configuration files.";
    };

    stateDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/launchpi/state";
      description = "Directory containing runtime state.";
    };

    cacheDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/cache/launchpi";
      description = "Directory containing cached assets.";
    };

    settings = lib.mkOption {
      type = lib.types.nullOr (lib.types.submodule {
        options = {
          devices = lib.mkOption {
            type = lib.types.listOf deviceSource;
            default = [];
            description = "Configured network devices or TOML documents containing devices.";
          };

          panels = lib.mkOption {
            type = lib.types.listOf panelSource;
            default = [];
            description = "Configured panels or TOML documents containing panels.";
          };

          values = lib.mkOption {
            type = lib.types.listOf userValue;
            default = [];
            description = "Configured user values.";
          };

          plugins = lib.mkOption {
            type = lib.types.attrsOf plugin;
            default = {};
            description = "Plugin instances indexed by their <type>.<name> filename stem.";
          };
        };
      });
      default = null;
      description = "Immutable Launchpi configuration generated as TOML files.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.settings == null || cfg.configDir == "/var/lib/launchpi/config";
        message = "services.launchpi.settings cannot be used with services.launchpi.configDir.";
      }
      {
        assertion = cfg.settings == null || lib.all (name: builtins.match "[a-z0-9][a-z0-9-]*\\.[a-z0-9][a-z0-9-]*" name != null) (lib.attrNames cfg.settings.plugins);
        message = "services.launchpi.settings.plugins keys must be <type>.<name> filename stems.";
      }
    ];

    users.groups = lib.optionalAttrs (cfg.group == "launchpi") {
      launchpi = {};
    };

    users.users = lib.optionalAttrs (cfg.user == "launchpi") {
      launchpi = {
        isSystemUser = true;
        inherit (cfg) group;
      };
    };

    networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [cfg.port];

    systemd.services.launchpi = {
      description = "Launchpi MIDI controller service";
      wantedBy = ["multi-user.target"];
      serviceConfig = {
        CacheDirectory = "launchpi";
        ExecStart = "${cfg.package}/bin/launchpi";
        User = cfg.user;
        Group = cfg.group;
        Restart = "on-failure";
        RestartSec = 5;
        StateDirectory = "launchpi";
        SupplementaryGroups = cfg.extraGroups;
      };
      environment = {
        LAUNCHPI_CACHE_DIR = cfg.cacheDir;
        LAUNCHPI_CONFIG_DIR = configDir;
        LAUNCHPI_CONFIG_READ_ONLY = lib.boolToString (cfg.settings != null);
        LAUNCHPI_DISCOVERY = lib.boolToString cfg.discovery;
        LAUNCHPI_HOST = cfg.host;
        LAUNCHPI_PORT = toString cfg.port;
        LAUNCHPI_STATE_DIR = cfg.stateDir;
      };
    };
  };
}
