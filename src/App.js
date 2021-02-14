import { useState, useEffect } from "react";
import {
  ChakraProvider,
  Center,
  Heading,
  Box,
  Stack,
} from "@chakra-ui/react";
import { FaFileVideo, FaFileCsv, FaFileImport } from "react-icons/fa"
import * as tauri from 'tauri/api/tauri'
import * as dialog from 'tauri/api/dialog'

import IButton from "./components/Button"
import IIConButton from "./components/IconButton"
import IInput from "./components/Input"
import IAlert from "./components/Alert";
import Regulator from "./components/Regulator";

function App() {
  const [errMsg, setErrMsg] = useState("");
  const [config, setConfig] = useState("");

  // 启动时加载默认配置
  useEffect(() => loadDefaultConfig(), []);

  function loadDefaultConfig() {
    tauri.promisified({ cmd: "LoadDefaultConfig" })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function loadConfig() {
    dialog.open({ filter: "json" }).then(path => {
      tauri.promisified({
        cmd: "LoadConfig",
        config_path: path,
      })
        .then(ok => setConfig(ok))
        .catch(err => setErrMsg(err));
    });
  }

  function saveConfig() {
    if (config.save_dir === "") {
      setErrMsg("请先确定保存根目录");
      return;
    }
    tauri.promisified({ cmd: "SaveConfig" })
      .catch(err => setErrMsg(err));
  }

  function setSaveDir() {
    dialog.open({ directory: true }).then(save_dir => {
      tauri.promisified({
        cmd: "SetSaveDir",
        save_dir,
      })
        .then(ok => setConfig(ok))
        .catch(err => setErrMsg(err));
    });
  }

  function setVideoPath() {
    dialog.open({
      filter: "avi,mp4,mkv",
      defaultPath: config.video_path.substr(0, config.video_path.lastIndexOf("\\") + 1)
    })
      .then(video_path => {
        tauri.promisified({
          cmd: "SetVideoPath",
          video_path,
        })
          .then(ok => setConfig(ok))
          .catch(err => setErrMsg(err));
      });
  }

  function setDAQPath() {
    dialog.open({
      filter: "lvm,xlsx",
      defaultPath: config.daq_path.substr(0, config.daq_path.lastIndexOf("\\") + 1)
    })
      .then(daq_path => {
        tauri.promisified({
          cmd: "SetDAQPath",
          daq_path,
        })
          .then(ok => setConfig(ok))
          .catch(err => setErrMsg(err));
      });
  }

  function setStartFrame(start_frame) {
    tauri.promisified({
      cmd: "SetStartFrame",
      start_frame: parseInt(start_frame),
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setStartRow(start_row) {
    tauri.promisified({
      cmd: "SetStartRow",
      start_row: parseInt(start_row),
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setPeakTemp(peak_temp) {
    tauri.promisified({
      cmd: "SetPeakTemp",
      peak_temp: parseFloat(peak_temp),
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setSolidThermalConductivity(solid_thermal_conductivity) {
    tauri.promisified({
      cmd: "SetSolidThermalConductivity",
      solid_thermal_conductivity: parseFloat(solid_thermal_conductivity),
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setSolidThermalDiffusivity(solid_thermal_diffusivity) {
    tauri.promisified({
      cmd: "SetSolidThermalDiffusivity",
      solid_thermal_diffusivity: parseFloat(solid_thermal_diffusivity),
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setAirThermalConductivity(air_thermal_conductivity) {
    tauri.promisified({
      cmd: "SetAirThermalConductivity",
      air_thermal_conductivity: parseFloat(air_thermal_conductivity),
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setCharacteristicLength(characteristic_length) {
    tauri.promisified({
      cmd: "SetCharacteristicLength",
      characteristic_length: parseFloat(characteristic_length),
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setRegulator(regulator) {
    tauri.promisified({
      cmd: "SetRegulator",
      regulator
    })
      .catch(err => setErrMsg(err));
  }

  return (
    <ChakraProvider>
      <Box h="800px" bg="#282828">
        <IAlert errMsg={errMsg} onClose={() => setErrMsg("")} />
        <IButton text="重置配置" onClick={loadDefaultConfig} hover="重置为您上一次保存的配置" />
        <IButton text="导入配置" onClick={loadConfig} />
        <IButton text="保存配置" onClick={saveConfig} />
        <Center>
          <Heading color="#689d6a">当前实验组：{config.case_name}</Heading>
        </Center>
        <Stack key={new Date().getTime()} >
          <IInput
            leftTag="保存根目录"
            hover="所有结果的保存根目录，该目录下将自动创建config、data和plots子目录分类保存处理结果"
            placeholder="保存所有结果的根目录"
            value={config.save_dir}
            element={<IIConButton icon={<FaFileImport />} onClick={setSaveDir} />}
          />
          <IInput
            leftTag="视频文件路径"
            value={config.video_path}
            element={<IIConButton icon={<FaFileVideo />} onClick={setVideoPath} />}
          />
          <IInput
            leftTag="数采文件路径"
            value={config.daq_path}
            element={<IIConButton icon={<FaFileCsv />} onClick={setDAQPath} />}
          />
          <IInput
            leftTag="起始帧数"
            value={config.frame_num > 0 ? config.start_frame : ""}
            mutable
            onBlur={setStartFrame}
            rightTag={config.frame_num > 0 ?
              `[${config.start_frame}, ${config.start_frame + config.frame_num}] / ${config.total_frames}` : ""}
          />
          <IInput
            leftTag="起始行数"
            value={config.frame_num > 0 ? config.start_row : ""}
            onBlur={setStartRow}
            mutable
            rightTag={config.frame_num > 0 ?
              `[${config.start_row}, ${config.start_row + config.frame_num}] / ${config.total_rows}` : ""}
          />
          <IInput
            leftTag="帧率"
            value={config.frame_rate > 0 ? config.frame_rate : ""}
            rightTag="Hz"
          />
          <IInput
            leftTag="峰值温度"
            value={!!config.peak_temp ? config.peak_temp.toPrecision(4) : ""}
            onBlur={setPeakTemp}
            mutable
            rightTag="°C"
          />
          <IInput
            leftTag="固体导热系数"
            value={!!config.solid_thermal_conductivity ? config.solid_thermal_conductivity.toPrecision(3) : ""}
            onBlur={setSolidThermalConductivity}
            mutable
            rightTag="W/(m·K)"
          />
          <IInput
            leftTag="固体热扩散系数"
            value={!!config.solid_thermal_diffusivity ? config.solid_thermal_diffusivity.toPrecision(4) : ""}
            rightTag="m2/s"
            onBlur={setSolidThermalDiffusivity}
            mutable
          />
          <IInput
            leftTag="气体导热系数"
            value={!!config.air_thermal_conductivity ? config.air_thermal_conductivity.toPrecision(3) : ""}
            onBlur={setAirThermalConductivity}
            mutable
            rightTag="W/(m·K)"
          />
          <IInput
            leftTag="特征长度"
            value={!!config.characteristic_length ? config.characteristic_length.toFixed(4) : ""}
            onBlur={setCharacteristicLength}
            mutable
            rightTag="m"
          />
          <Regulator regulator={config.regulator} onSubmit={setRegulator} />
        </Stack>
      </Box>
    </ChakraProvider >
  )
}

export default App;
