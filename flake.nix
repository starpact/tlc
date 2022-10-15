{
  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs, flake-utils }:
    let
      pkgs = import nixpkgs { system = "x86_64-linux"; };
      libraries = with pkgs;[
        dbus.lib
        cairo
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
        ffmpeg
        glib
        gtk3
        libsoup
        openssl
        pkg-config
        sqlite
        webkitgtk
        wget
      ];
    in
    {
      devShells.x86_64-linux.default = pkgs.mkShell {
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
    };
}
