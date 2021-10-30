import { invoke } from "@tauri-apps/api/tauri";

function App() {
  const f = () => invoke<String>("get_save_info")
    .then((msg?: String) => console.log(msg))
    .catch((err?: String) => console.error(err));

  return (
    <div>
      Hello TLC
      <button onClick={f}></button>
    </div>
  )
}

export default App
