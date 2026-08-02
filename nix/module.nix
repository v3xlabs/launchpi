{self}: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.launchpi;
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

    openFirewall = lib.mkEnableOption "opening the Launchpi port in the firewall";

    configDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/launchpi/config";
      description = "Directory containing Launchpi configuration files.";
    };

    stateDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/launchpi/state";
      description = "Directory containing Launchpi runtime state.";
    };

    cacheDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/cache/launchpi";
      description = "Directory containing Launchpi cached assets.";
    };
  };

  config = lib.mkIf cfg.enable {
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
      };
      environment = {
        LAUNCHPI_CACHE_DIR = cfg.cacheDir;
        LAUNCHPI_CONFIG_DIR = cfg.configDir;
        LAUNCHPI_HOST = cfg.host;
        LAUNCHPI_PORT = toString cfg.port;
        LAUNCHPI_STATE_DIR = cfg.stateDir;
      };
    };
  };
}
