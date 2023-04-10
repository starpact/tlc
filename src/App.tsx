import React, { useEffect, useState } from 'react';

import * as tauri from '@tauri-apps/api/tauri';
import TextField from '@mui/material/TextField';
import * as dialog from '@tauri-apps/api/dialog';
import { Stack } from '@mui/system';
import { Button } from '@mui/material';

interface Array2 {
  dim: [number, number],
  data: number[],
}

function App() {
  const [name, setName] = useState("");
  const [saveRootDir, setSaveRootDir] = useState("");
  const [videoPath, setVideoPath] = useState("");
  const [nframes, setNFrames] = useState<number | null>(null);
  const [videoFrameRate, setVideoFrameRate] = useState<number | null>(null);
  const [videoShape, setVideoShape] = useState<[number, number] | null>(null);
  const [daqPath, setDaqPath] = useState<string>("");
  const [daqData, setDaqData] = useState<Array2 | null>(null);

  async function apiGetName() {
    const name = await tauri.invoke<string>("get_name");
    setName(name);
    return name;
  }
  async function apiSetName() {
    await tauri.invoke<void>("set_name", { name });
  }

  async function apiGetSaveRootDir() {
    const saveRootDir = await tauri.invoke<string>("get_save_root_dir");
    setSaveRootDir(saveRootDir);
    return saveRootDir;
  }
  async function apiSetSaveRootDir() {
    await tauri.invoke<void>("set_save_root_dir", { saveRootDir });
  }
  async function chooseSaveRootDir() {
    const dir = await dialog.open({
      title: "Choose directory where all results are saved",
      defaultPath: saveRootDir,
      directory: true,
    });
    if (typeof dir == "string") setSaveRootDir(dir);
  }

  async function apiGetVideoPath() {
    const videoPath = await tauri.invoke<string>("get_video_path");
    setVideoPath(videoPath);
    return videoPath;
  }
  async function apiSetVideoPath() {
    if (videoPath === "") return;
    await tauri.invoke<void>("set_video_path", { videoPath });
    setNFrames(await tauri.invoke<number>("get_video_nframes"));
    setVideoFrameRate(await tauri.invoke<number>("get_video_frame_rate"));
    setVideoShape(await tauri.invoke<[number, number]>("get_video_shape"));
  }
  async function chooseVideoPath() {
    const path = await dialog.open({
      title: "Choose video",
      defaultPath: videoPath,
      directory: false,
      filters: [{ name: "video filter", extensions: ["avi", "mp4"] }]
    });
    if (typeof path == "string") setVideoPath(path);
  }

  async function apiGetDaqPath() {
    const daqPath = await tauri.invoke<string>("get_daq_path");
    setDaqPath(daqPath);
    return daqPath;
  }
  async function apiSetDaqPath() {
    await tauri.invoke<void>("set_daq_path", { daqPath });
    setDaqData(await tauri.invoke<Array2>("get_daq_data"));
  }
  async function chooseDaqPath() {
    const path = await dialog.open({
      title: "Choose DAQ",
      defaultPath: daqPath,
      directory: false,
      filters: [{ name: "daq filter", extensions: ["lvm", "xls", "xlsx"] }]
    });
    if (typeof path == "string") setDaqPath(path);
  }

  let loaded = false;
  useEffect(() => {
    if (loaded) return;
    loaded = true;
    Promise.allSettled([
      apiGetName(),
      apiGetSaveRootDir(),
      apiGetVideoPath(),
      apiGetDaqPath(),
    ]).then(dbg);
  }, []);

  useEffect(debounce("name", () => { apiSetName().catch(dbg); }), [name]);
  useEffect(() => { apiSetSaveRootDir().catch(dbg); }, [saveRootDir]);
  useEffect(() => { apiSetVideoPath().catch(dbg); }, [videoPath]);
  useEffect(() => { apiSetDaqPath().catch(dbg); }, [daqPath]);

  return (
    <div className="App">
      <Stack spacing={1}>
        <TextField
          label="Name"
          value={name}
          onChange={event => setName(event.target.value)}
        />
        <Stack direction={"row"}>
          <TextField label="Save Root Dir" value={saveRootDir} />
          <Button variant="outlined" onClick={chooseSaveRootDir}>Choose</Button>
        </Stack>
        <Stack direction={"row"}>
          <TextField label="Video Path" value={videoPath} />
          <Button variant="outlined" onClick={chooseVideoPath}>Choose</Button>
        </Stack>
        <Stack direction={"row"}>
          <TextField label="DAQ Path" value={daqPath} />
          <Button variant="outlined" onClick={chooseDaqPath}>Choose</Button>
        </Stack>
        <TextField value={`\
nframes: ${nframes ?? "_"}
frame_rate: ${videoFrameRate ?? "_"}
video_shape: ${videoShape ?? "_"}`
        } multiline />
        <TextField value={`daq_data_dim: ${daqData?.dim ?? "_"}`} multiline />
      </Stack>
    </div >
  );
}

function dbg(e: any) {
  console.log("[DEBUG]", e);
}

let versions = new Map<string, number>;
function debounce(key: string, callback: () => void, delayMillis?: number) {
  return () => {
    let version = versions.get(key);
    if (version !== undefined) {
      version += 1;
    } else {
      version = 0;
    }
    versions.set(key, version);
    setTimeout(() => {
      if (version === versions.get(key)) callback();
    }, delayMillis ?? 500)
  }
}

export default App;
