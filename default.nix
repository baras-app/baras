{
  appimageTools,
  lib,
  fetchurl,
}:

let
  version = "v2026.11.23";
  pname = "baras";

  src = fetchurl {
    url = "https://github.com/baras-app/${pname}/releases/download/${version}/BARAS_2026.11.23_amd64.AppImage";
    hash = "sha256:sha256-MWicFCcdsbETmn8sFz6D+2YPxNIx5+xe5BtHGJao6Ng=";
  };

in
appimageTools.wrapType2 {
  inherit pname version src;
  extraPkgs = pkgs: [ pkgs.libgpg-error ];

  meta = {
      description = "Baras is a SWTOR Baras written in Rust";
      longDescription = ''The Battle Analysis and Raid Assessment System (BARAS) is the ultimate companion for SWTOR endgame content
      '';
      homepage = "https://baras-app.github.io/";
      license = lib.licenses.mit;
      maintainers = with lib.maintainers; [ pdubs ];
      platforms = lib.platforms.linux;
    };
}
