{self}: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.launchpi;
  toml = pkgs.formats.toml {};
  settingsDir = pkgs.runCommand "launchpi-config" {} ''
    mkdir -p "$out/plugins"
    ${lib.optionalString (cfg.settings.devices != null) ''
      ln -s ${toml.generate "devices.toml" cfg.settings.devices} "$out/devices.toml"
    ''}
    ${lib.optionalString (cfg.settings.panels != null) ''
      ln -s ${toml.generate "panels.toml" cfg.settings.panels} "$out/panels.toml"
    ''}
    ${lib.optionalString (cfg.settings.values != null) ''
      ln -s ${toml.generate "values.toml" cfg.settings.values} "$out/values.toml"
    ''}
    ${lib.concatStringsSep "\n" (lib.mapAttrsToList (name: value: ''
        ln -s ${toml.generate "${name}.toml" value} "$out/plugins/${name}.toml"
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
            type = lib.types.nullOr (lib.types.attrsOf lib.types.anything);
            default = null;
            description = "Contents of devices.toml.";
          };

          panels = lib.mkOption {
            type = lib.types.nullOr (lib.types.attrsOf lib.types.anything);
            default = null;
            description = "Contents of panels.toml.";
          };

          values = lib.mkOption {
            type = lib.types.nullOr (lib.types.attrsOf lib.types.anything);
            default = null;
            description = "Contents of values.toml.";
          };

          plugins = lib.mkOption {
            type = lib.types.attrsOf (lib.types.attrsOf lib.types.anything);
            default = {};
            description = "Plugin documents indexed by their <type>.<name> filename stem.";
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
