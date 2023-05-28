{
  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      stdenv = pkgs.llvmPackages_15.stdenv;
      libraries = with pkgs;[ llvmPackages_15.libclang.lib ];
      packages = with pkgs; [
        cargo-nextest
        cargo-tauri
        ffmpeg-full
        nodejs
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
            '';
        };
      };
    };
}
