{
  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      stdenv = pkgs.llvmPackages_14.stdenv;
      libraries = with pkgs;[
        dbus.lib
        cairo
        ffmpeg-full
        gdk-pixbuf
        glib.out
        gtk3
        llvmPackages_14.libclang.lib
        pango.out
        openssl.out
        webkitgtk
      ];
      packages = with pkgs; [
        cargo-nextest
        cargo-tauri
        dbus
        ffmpeg-full
        glib
        gtk3
        libsoup
        mold
        nodejs
        openssl
        pkg-config
        sqlite
        webkitgtk
      ];
    in
    {
      devShells = {
        x86_64-linux.default = pkgs.mkShell.override { inherit stdenv; } {
          buildInputs = packages;
          shellHook =
            let
              joinLibs = libs: builtins.concatStringsSep ":" (builtins.map (x: "${x}/lib") libs);
              libs = joinLibs libraries;
            in
            ''
              export LD_LIBRARY_PATH=${libs}:$LD_LIBRARY_PATH
              export RUST_BACKTRACE=1
            '';
        };
      };
    };
}