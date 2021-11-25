import { invoke } from "@tauri-apps/api/tauri";
import { useState } from "react";

function App() {
  const [image, setImage] = useState<string>();

  function get_save_root_dir() {
    invoke<string>("get_save_root_dir")
      .then((msg) => console.log(msg))
      .catch((err: string) => console.error(err));
  }

  function get_frame() {
    invoke<string>("get_frame", { frameIndex: 2000 })
      .then((msg) => setImage(msg))
      .catch((err: string) => console.error(err));
  }

  interface VideoMeta {
    path: string,
    frame_rate: number,
    total_frames: number,
    shape: Uint32Array,
  }

  function get_video_meta() {
    invoke<VideoMeta>("get_video_meta")
      .then((videoMeta) => console.log(videoMeta))
      .catch((err: string) => console.error(err));
  }

  function set_video_path() {
    invoke<VideoMeta>("set_video_path", { path: "fake" })
      .then((videoMeta) => console.log(videoMeta))
      .catch((err: string) => console.error(err));
  }

  interface DAQMeta {
    path: string,
    total_rows: number,
  }

  function get_daq_meta() {
    invoke<DAQMeta>("get_daq_meta")
      .then((daqMeta) => console.log(daqMeta))
      .catch((err: string) => console.error(err));
  }


  function set_daq_path() {
    invoke<DAQMeta>("set_daq_path", { path: "fake.lvm" })
      .catch((err: string) => console.error(err));
  }

  function get_daq() {
    invoke<string>("get_daq")
      .then((daq) => console.log(daq))
      .catch((err: string) => console.error(err));
  }

  function set_start_frame() {
    invoke<number>("set_start_frame", { startFrame: 1 })
      .then((cal_num) => console.log(cal_num))
      .catch((err: string) => console.error(err));
  }

  function set_start_row() {
    invoke<number>("set_start_row", { startRow: 1 })
      .then((cal_num) => console.log(cal_num))
      .catch((err: string) => console.error(err));
  }


  function set_area() {
    invoke<void>("set_area", { area: [200, 200, 800, 1000] }).catch((err: string) => console.error(err));
  }

  function set_filter_method() {
    invoke<void>("set_filter_method", { filterMethod: { Median: 5 } })
      .catch((err: string) => console.error(err));
  }

  return (
    <div>
      <div>Hello TLC</div>
      <br />
      <button onClick={get_save_root_dir}>get_save_root_dir</button>
      <br />
      <button onClick={get_frame}>get_frame</button>
      <br />
      <button onClick={get_video_meta}>get_video_meta</button>
      <br />
      <button onClick={set_video_path}>set_video_path</button>
      <br />
      <button onClick={get_daq}>get_daq</button>
      <br />
      <button onClick={get_daq_meta}>get_daq_meta</button>
      <br />
      <button onClick={set_daq_path}>set_daq_path</button>
      <br />
      <button onClick={set_start_frame}>set_start_frame</button>
      <br />
      <button onClick={set_start_row}>set_start_row</button>
      <br />
      <button onClick={set_area}>set_area</button>
      <br />
      <button onClick={set_filter_method}>set_filter_method</button>
      <br />
      <img alt="frame" src={`data:image/jpeg;base64,${image}`} />
    </div>
  )
}

export default App
