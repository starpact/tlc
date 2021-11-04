import { invoke } from "@tauri-apps/api/tauri";

function App() {
  let path = "fake video path";

  function get_save_info() {
    invoke<String>("get_save_info")
      .then((msg?: String) => console.log(msg))
      .catch((err?: String) => console.error(err));
  }

  function set_video_path() {
    invoke<String>("set_video_path", { path })
      .catch((err?: String) => console.error(err));
    path += "_xxx";
  }

  function get_frame() {
    invoke<Number>("get_frame", {
      frameIndex: 5
    })
      .then((msg?: Number) => console.log(msg))
      .catch((err?: String) => console.error(err));
  }

  return (
    <div>
      <div>Hello TLC</div>
      <br />
      <button onClick={get_save_info}>get_save_info</button>
      <br />
      <button onClick={set_video_path}>set_video_path</button>
      <br />
      <button onClick={get_frame}>get_frame</button>
      <br />
    </div >
  )
}

export default App
