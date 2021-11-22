import { invoke } from "@tauri-apps/api/tauri";
import { useState } from "react";

function App() {
  const [image, setImage] = useState<string>();

  function get_save_info() {
    invoke<string>("get_save_info")
      .then((msg?: string) => console.log(msg))
      .catch((err?: string) => console.error(err));
  }

  function set_video_path() {
    invoke<string>("set_video_path", { path: "fake" })
      .catch((err?: string) => console.error(err));
  }

  function set_daq_path() {
    invoke<string>("set_daq_path", { path: "fake.lvm" })
      .catch((err?: string) => console.error(err));
  }

  function get_frame() {
    invoke<string>("get_frame", { frameIndex: 2000 })
      .then((msg?: string) => setImage(msg))
      .catch((err?: string) => console.error(err));
  }

  function set_start_frame() {
    invoke<void>("set_start_frame", { startFrame: 1 })
      .catch((err?: string) => console.error(err));
  }

  function set_start_row() {
    invoke<void>("set_start_row", { startRow: 1 })
      .catch((err?: string) => console.error(err));
  }


  function set_area() {
    invoke<void>("set_area", { area: [100, 100, 800, 1000] })
      .catch((err?: string) => console.error(err));
  }

  return (
    <div>
      <div>Hello TLC</div>
      <br />
      <button onClick={get_save_info}>get_save_info</button>
      <br />
      <button onClick={set_video_path}>set_video_path</button>
      <br />
      <button onClick={set_daq_path}>set_daq_path</button>
      <br />
      <button onClick={get_frame}>get_frame</button>
      <br />
      <button onClick={set_start_frame}>set_start_frame</button>
      <br />
      <button onClick={set_start_row}>set_start_row</button>
      <br />
      <button onClick={set_area}>set_area</button>
      <br />
      <img alt="frame" src={`data:image/jpeg;base64,${image}`} />
    </div>
  )
}

export default App
