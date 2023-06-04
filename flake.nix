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
      llvmPackages = pkgs.llvmPackages_16;
    in
    {
      devShells = {
        x86_64-linux.default = pkgs.mkShell.override { stdenv = llvmPackages.stdenv; } {
          buildInputs = with pkgs; [
            (rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
              extensions = [ "rust-src" "rust-analyzer" ];
              targets = [ "x86_64-unknown-linux-gnu" "x86_64-pc-windows-gnu" ];
            }))
            cargo-nextest
            cargo-tauri
            ffmpeg-full
            nodejs
            openssl
            pkg-config
            webkitgtk
          ];
          shellHook =
            ''
              export LIBCLANG_PATH=${llvmPackages.libclang.lib}/lib
              export RUST_BACKTRACE=1
              export CARGO_TERM_COLOR=always
            '';
        };
      };
    };
}
