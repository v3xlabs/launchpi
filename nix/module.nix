{self}: {
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.launchpi;
in {
  options.services.launchpi = {
    enable = lib.mkEnableOption "the Launchpi user service";

    package = lib.mkPackageOption self.packages.${pkgs.stdenv.hostPlatform.system} "launchpi" {};

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
  };

  config = lib.mkIf cfg.enable {
    systemd.user.services.launchpi = {
      description = "Launchpi MIDI controller service";
      wantedBy = [ "default.target" ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/launchpi";
        Restart = "on-failure";
        RestartSec = 5;
      };
      environment = {
        LAUNCHPI_HOST = cfg.host;
        LAUNCHPI_PORT = toString cfg.port;
      };
    };
  };
}
