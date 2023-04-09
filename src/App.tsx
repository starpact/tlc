import React, { useEffect, useState } from 'react';

import * as tauri from '@tauri-apps/api/tauri';
import TextField from '@mui/material/TextField';
import * as dialog from '@tauri-apps/api/dialog';
import { Stack } from '@mui/system';
import { Button } from '@mui/material';

function App() {
  const [name, setName] = useState("");
  const [saveRootDir, setSaveRootDir] = useState("");
  const [videoPath, setVideoPath] = useState("");
  const [nframes, setNFrames] = useState<number | null>(null);
  const [videoFrameRate, setVideoFrameRate] = useState<number | null>(null);
  const [videoShape, setVideoShape] = useState<[number, number] | null>(null);

  async function apiGetName() {
    const name = await tauri.invoke<string>("get_name");
    setName(name);
    return name;
  }
  async function apiSetName() {
    if (name == "") return;
    try { tauri.invoke<void>("set_name", { name }); } catch (e) { eprint(e) }
  }


  async function apiGetSaveRootDir() {
    const saveRootDir = await tauri.invoke<string>("get_save_root_dir");
    setSaveRootDir(saveRootDir);
    return saveRootDir;
  }
  async function apiSetSaveRootDir() {
    if (saveRootDir == "") return;
    try { await tauri.invoke<void>("set_save_root_dir", { saveRootDir }); } catch (e) { eprint(e) }
  }
  function chooseSaveRootDir() {
    dialog.open({
      title: "选择保存结果的根目录",
      defaultPath: saveRootDir,
      directory: true,
    }).then(dir => { if (typeof dir == "string") setSaveRootDir(dir); })
  }

  async function apiGetVideoPath() {
    const videoPath = await tauri.invoke<string>("get_video_path");
    setVideoPath(videoPath);
    return videoPath;
  }
  async function apiSetVideoPath() {
    if (videoPath == "") return;
    try {
      await tauri.invoke<void>("set_video_path", { videoPath });
      setNFrames(await tauri.invoke<number>("get_video_nframes"));
      setVideoFrameRate(await tauri.invoke<number>("get_video_frame_rate"));
      setVideoShape(await tauri.invoke<[number, number]>("get_video_shape"));
    } catch (e) {
      eprint(e);
    }
  }
  function chooseVideoPath() {
    dialog.open({
      title: "选择视频",
      defaultPath: videoPath,
      directory: false,
      filters: [{ name: "video filter", extensions: ["avi", "mp4"] }]
    }).then(path => { if (typeof path == "string") setVideoPath(path); })
  }

  let loaded = false;
  useEffect(() => {
    if (loaded) return;
    loaded = true;
    Promise.allSettled([
      apiGetName(),
      apiGetSaveRootDir(),
      apiGetVideoPath(),
    ]).then(eprint);
  }, []);

  useEffect(() => { apiSetSaveRootDir(); }, [saveRootDir]);
  useEffect(() => { apiSetVideoPath(); }, [videoPath]);

  return (
    <div className="App">
      <TextField
        label="Name"
        value={name}
        onChange={event => setName(event.target.value)}
        onBlur={apiSetName}
        onKeyDown={event => { if (event.key == "Enter") apiSetName(); }}
      />
      <Stack direction={"row"}>
        <TextField label="Save Root Dir" value={saveRootDir} />
        <Button variant="outlined" onClick={chooseSaveRootDir}>Choose</Button>
      </Stack>
      <Stack direction={"row"}>
        <TextField label="Video Path" value={videoPath} />
        <Button variant="outlined" onClick={chooseVideoPath}>Choose</Button>
      </Stack>
      <TextField value={`\
nframes: ${nframes ?? "_"}
frame_rate: ${videoFrameRate ?? "_"}
video_shape: [${videoShape ?? "_, _"}]`
      } multiline />
    </div >
  );
}

function eprint(e: any) {
  console.log(e);
}

export default App;
