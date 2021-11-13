# Transient Liquid Crystal Experiment Data Processing

## Architecture

Built with [tauri](https://tauri.studio/en/):

* Frontend: [react](https://reactjs.org/) + [chakra](https://chakra-ui.com/) + [echarts](https://echarts.apache.org/en/index.html)
* Backend: [rust](https://www.rust-lang.org/)([tokio](https://tokio.rs/) + [rayon](https://github.com/rayon-rs/rayon) + [ndarray](https://github.com/rust-ndarray/ndarray)) + [ffmpeg](https://www.ffmpeg.org/)

## User Manual

## Tmp Notes

[Taking Advantage of Auto-Vectorization in Rust](https://www.nickwilcox.com/blog/autovec)

[Async: What is blocking?](https://ryhl.io/blog/async-what-is-blocking/)

[FFmpeg: Difference Between Frames and Packets](https://stackoverflow.com/questions/53574798/difference-between-frames-and-packets-in-ffmpeg)

[FFmpeg: multithread decoding](https://www.cnblogs.com/TaigaCon/p/10220356.html)
