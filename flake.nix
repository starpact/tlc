{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      pkgs = import nixpkgs {
        system = "x86_64-linux";
        overlays = [ (import rust-overlay) ];
      };
      stdenv = pkgs.llvmPackages_16.stdenv;
      libraries = with pkgs;[
        dbus.lib
        cairo
        ffmpeg-full
        gdk-pixbuf
        glib.out
        gtk3
        llvmPackages_16.libclang.lib
        openssl.out
        webkitgtk
      ];
      packages = with pkgs; [
        (rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
          targets = [ "x86_64-unknown-linux-gnu" "x86_64-pc-windows-gnu" ];
        }))
        cargo-nextest
        cargo-tauri
        dbus
        ffmpeg-full
        glib
        gtk3
        libsoup
        nodejs
        openssl
        pkg-config
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
              export CARGO_TERM_COLOR=always
            '';
        };
      };
    };
}
