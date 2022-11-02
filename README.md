# Transient Liquid Crystal Experiment Data Processing

Built with [Tauri](https://tauri.app).

## Architecture
![arch](.github/assets/tlc_architecture.png)

## Data Dependencies
### Setting Mapping to Runtime Data 
```mermaid
flowchart
    video_path[Video Path] --> packets[Packets]
    video_path --> decoder_manager[Decoder Manager]
    packets --> green2[Green2]
    decoder_manager --> green2
    start_frame[Start Frame] --> green2
    area[Area] --> green2
    area --> interpolator
    green2 --> gmax_frame_indexes[Gmax Frame Indexes]
    daq_path[DAQ Path] --> daq_raw[DAQ Raw]
    daq_raw --> interpolator
    start_row[Start Row] --> interpolator
    thermocouples[Thermocouples] --> interpolator[Interpolator]
    interp_method[Interpolation Method] --> interpolator
    filter_method[Filter Method] --> gmax_frame_indexes
    iteration_method[Iteration Method] --> nu2[Nu2]
    gmax_frame_indexes --> nu2
    interpolator --> nu2
    physical_param[Physical Parameters] --> nu2
    style video_path fill:#bbf
    style daq_path fill:#bbf
    style start_frame fill:#bbf
    style start_row fill:#bbf
    style area fill:#bbf
    style thermocouples fill:#bbf
    style interp_method fill:#bbf
    style filter_method fill:#bbf
    style iteration_method fill:#bbf
    style physical_param fill:#bbf
```

### Logic Dependencies within Setting
```mermaid
flowchart
    video_path[Video Path] --> area[Area]
    video_path[Video Path] --> start_frame[Start Frame]
    video_path[Video Path] --> start_row[Start Row]
    video_path[Video Path] --> thermocouples[Thermocouples]
    daq_path[DAQ Path] --> start_frame
    daq_path[DAQ Path] --> start_row
    daq_path[DAQ Path] --> thermocouples
    style video_path fill:#bbf
    style daq_path fill:#bbf
    style start_frame fill:#bbf
    style start_row fill:#bbf
    style area fill:#bbf
    style thermocouples fill:#bbf
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

# install `ffmpeg` via `vcpkg`, need to compile for about 20 mins

# let vcpkg expose ffmpeg headers

# install `llvm`

# install `cargo-vcpkg`
```

## References
- [Taking Advantage of Auto-Vectorization in Rust](https://www.nickwilcox.com/blog/autovec)
- [Async: What is blocking?](https://ryhl.io/blog/async-what-is-blocking/)
- [FFmpeg: Difference Between Frames and Packets](https://stackoverflow.com/questions/53574798/difference-between-frames-and-packets-in-ffmpeg)
- [FFmpeg: multithread decoding](https://www.cnblogs.com/TaigaCon/p/10220356.html)
- [Data as a mediator between computation and state](https://www.tedinski.com/2018/08/28/using-data-to-mutate-state.html)
- [Matklad's reply on reddit](https://www.reddit.com/r/rust/comments/uf7yoy/comment/i6s4b8x/)
