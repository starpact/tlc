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


### Misc

#### Smooth Progress Bar
When user drags the progress bar quickly, the decoding can not keep up and there will be a significant lag. Actually, we do not have to decode every frames, and the key is how to give up decoding some frames properly. The naive solution to avoid too much backlog is maintaining the number of pending tasks and directly abort current decoding if it already exceeds the limit. But FIFO is not perfect for this use case because it's better to give
priority to newer frames, e.g. we should at least guarantee decoding the frame where the progress bar **stops**.
`ring_buffer` is used to automatically eliminate the oldest frame to limit the
number of backlog frames.
```rust
thread_pool: ThreadPool,
ring_buffer: ArrayQueue<oneshot::Sender<T>>,
sem: Semaphore,
```

## References
- [Taking Advantage of Auto-Vectorization in Rust](https://www.nickwilcox.com/blog/autovec)
- [Async: What is blocking?](https://ryhl.io/blog/async-what-is-blocking/)
- [FFmpeg: Difference Between Frames and Packets](https://stackoverflow.com/questions/53574798/difference-between-frames-and-packets-in-ffmpeg)
- [FFmpeg: multithread decoding](https://www.cnblogs.com/TaigaCon/p/10220356.html)
- [Matklad's reply on reddit](https://www.reddit.com/r/rust/comments/uf7yoy/comment/i6s4b8x/)
