import { invoke } from "@tauri-apps/api/tauri";

function App() {
  // invoke("my_custom_command", { number: 7 })
  //   .then(msg => console.log(msg))
  //   .catch(err => console.error(err));
  invoke("block").then(() => console.log("aaa"));

  return (
    <div>
      Hello TLC
    </div>
  )
}

export default App
