# Transient Liquid Crystal Experiment Data Processing

## Development
### Linux
- install rust nightly-x86_64-unknown-linux-gnu toolchain
- install [Nix](https://nixos.org/) and enable [Flake](https://nixos.wiki/wiki/Flakes), this will manage all other dependencies.
```sh
# enter the environment
nix develop # or use direnv
```
Cross compile to Windows(TODO).

### Windows(TODO)
- install rust nightly-x86_64-pc-windows-msvc toolchain
```sh
# install `ffmpeg` via `vcpkg`, need to compile for about 20 mins

# let vcpkg expose ffmpeg headers

# install `llvm`

# install `cargo-vcpkg`
```

## Architecture
```mermaid
flowchart
    video_path[Video Path] --> packets((Packets))
    video_path --> decoder((Decoder))
    packets --> green2((Green2))
    decoder --> green2
    start_frame[Start Frame] --> green2
    area[Area] --> green2
    area --> interpolator
    green2 --> gmax_frame_indexes((Gmax Frame Indexes))
    daq_path[DAQ Path] --> daq_raw[DAQ Raw]
    daq_raw --> interpolator
    start_row[Start Row] --> interpolator
    thermocouples[Thermocouples] --> interpolator((Interpolator))
    interp_method[Interpolation Method] --> interpolator
    filter_method[Filter Method] --> gmax_frame_indexes
    iter_method[Iter Method] --> nu2((Nu2))
    gmax_frame_indexes --> nu2
    interpolator --> nu2
    physical_param[Physical Parameters] --> nu2
```

Some `input`s depend on others, these relationships need to be maintained manually.
```mermaid
flowchart
    video_path[Video Path] --> area[Area]
    video_path[Video Path] --> start_index[Start Index]
    video_path[Video Path] --> thermocouples[Thermocouples]
    daq_path[DAQ Path] --> start_index
    daq_path[DAQ Path] --> thermocouples
    area --> thermocouples
```
