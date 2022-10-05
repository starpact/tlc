{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        libraries = with pkgs;[
          dbus.lib
          cairo
          ffmpeg
          gdk-pixbuf
          glib.out
          gtk3
          llvmPackages_14.libclang.lib
          openssl.out
          webkitgtk
        ];
        packages = with pkgs; [
          cargo-tauri
          curl
          dbus
          glib
          gtk3
          libsoup
          openssl
          pkg-config
          webkitgtk
          wget
        ];
      in
      {
        devShell = pkgs.mkShell {
          buildInputs = packages;
          shellHook =
            let
              joinLibs = libs: builtins.concatStringsSep ":" (builtins.map (x: "${x}/lib") libs);
              libs = joinLibs libraries;
            in
            ''
              export LD_LIBRARY_PATH=${libs}:$LD_LIBRARY_PATH
            '';
        };
      });
}
