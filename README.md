# Transient Liquid Crystal Experiment Data Processing

Built with [Tauri](https://tauri.app).

## Architecture
![arch](.github/assets/tlc_architecture.png)

```mermaid
flowchart TD
    A[Frontend] -->|IPC Request| B[Tauri Core]
    B -->|IPC Response| A
    B --> C((Query))
    C -->|Read| E[Setting]
    C -->|Read| F[Runtime Data]
    B --> D((Command))
    D -->|Read/Write| E
    D -->|Trigger Once| G[Controller]
    G -->|Read|E
    G -->|Read|F
    G -->|Reconcile|H[Compute]
    H -->|Write| F
    H -->|Succeed?| G
```

## Development
### Linux
First you need to installed [Nix](https://nixos.org/) and enable [Flake](https://nixos.wiki/wiki/Flakes).
```bash
# enter the environment
nix develop # or use direnv

# run
cargo tauri dev

# build
cargo tauri build
```
Cross compile to Windows(TODO).

### Windows(TODO)
```sh
# install rust x86_64-pc-windows-msvc toolchain

# install tauri-cli
cargo install tauri-cli

# install `ffmpeg` via `vcpkg`, need to compile about 20 mins

# let vcpkg expose ffmpeg headers

# install `llvm`

# install `cargo-vcpkg`
```

## Misc
- [Taking Advantage of Auto-Vectorization in Rust](https://www.nickwilcox.com/blog/autovec)
- [Async: What is blocking?](https://ryhl.io/blog/async-what-is-blocking/)
- [FFmpeg: Difference Between Frames and Packets](https://stackoverflow.com/questions/53574798/difference-between-frames-and-packets-in-ffmpeg)
- [FFmpeg: multithread decoding](https://www.cnblogs.com/TaigaCon/p/10220356.html)
