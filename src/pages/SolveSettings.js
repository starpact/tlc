import { Stack } from "@chakra-ui/react";
import * as tauri from 'tauri/api/tauri';

import IInput from "../components/Input";

import Regulator from "../components/Regulator";
import SelectFilter from "../components/SelectFilter";
import SelectInterp from "../components/SelectInterp";
import SelectIteration from "../components/SelectIteration";

function SolveSettings({ config, setConfig, setErrMsg }) {

  function setPeakTemp(peak_temp) {
    if (Math.abs(peak_temp - config.peak_temp) < 1e-5) return;
    tauri.promisified({
      cmd: "setPeakTemp",
      body: peak_temp,
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setSolidThermalConductivity(solid_thermal_conductivity) {
    if (Math.abs(solid_thermal_conductivity - config.solid_thermal_conductivity) < 1e-5) return;
    tauri.promisified({
      cmd: "setSolidThermalConductivity",
      body: solid_thermal_conductivity,
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setSolidThermalDiffusivity(solid_thermal_diffusivity) {
    if (Math.abs(solid_thermal_diffusivity - config.solid_thermal_diffusivity) < 1e-5) return;
    tauri.promisified({
      cmd: "setSolidThermalDiffusivity",
      body: solid_thermal_diffusivity,
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setAirThermalConductivity(air_thermal_conductivity) {
    if (Math.abs(air_thermal_conductivity - config.air_thermal_conductivity) < 1e-5) return;
    tauri.promisified({
      cmd: "setAirThermalConductivity",
      body: air_thermal_conductivity,
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setCharacteristicLength(characteristic_length) {
    if (Math.abs(characteristic_length - config.characteristic_length) < 1e-5) return;
    tauri.promisified({
      cmd: "setCharacteristicLength",
      body: characteristic_length,
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setRegulator(regulator) {
    tauri.promisified({
      cmd: "setRegulator",
      body: regulator,
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setFilterMethod(filter_method) {
    tauri.promisified({
      cmd: "setFilterMethod",
      body: filter_method,
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setInterpMethod(interp_method) {
    tauri.promisified({
      cmd: "setInterpMethod",
      body: interp_method,
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setIterationMethod(iteration_method) {
    tauri.promisified({
      cmd: "setIterationMethod",
      body: iteration_method,
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err))
  }

  return (
    <Stack>
      <IInput
        leftTag="峰值温度"
        value={!!config.peak_temp ? config.peak_temp.toPrecision(4) : ""}
        onBlur={v => setPeakTemp(parseFloat(v))}
        mutable
        rightTag="°C"
      />
      <IInput
        leftTag="固体导热系数"
        value={!!config.solid_thermal_conductivity ? config.solid_thermal_conductivity.toPrecision(3) : ""}
        onBlur={v => setSolidThermalConductivity(parseFloat(v))}
        mutable
        rightTag="W/(m·K)"
      />
      <IInput
        leftTag="固体热扩散系数"
        value={!!config.solid_thermal_diffusivity ? config.solid_thermal_diffusivity.toPrecision(4) : ""}
        rightTag="m2/s"
        onBlur={v => setSolidThermalDiffusivity(parseFloat(v))}
        mutable
      />
      <IInput
        leftTag="气体导热系数"
        value={!!config.air_thermal_conductivity ? config.air_thermal_conductivity.toPrecision(3) : ""}
        onBlur={v => setAirThermalConductivity(parseFloat(v))}
        mutable
        rightTag="W/(m·K)"
      />
      <IInput
        leftTag="特征长度"
        value={!!config.characteristic_length ? config.characteristic_length.toFixed(4) : ""}
        onBlur={v => setCharacteristicLength(parseFloat(v))}
        mutable
        rightTag="m"
      />
      <Regulator
        regulator={config.regulator}
        onSubmit={setRegulator}
      />
      <SelectFilter
        value={config.filter_method}
        onSubmit={setFilterMethod}
        setErrMsg={setErrMsg}
      />
      <SelectInterp
        value={config.interp_method}
        onSubmit={setInterpMethod}
        setErrMsg={setErrMsg}
      />
      <SelectIteration
        value={config.iteration_method}
        onSubmit={setIterationMethod}
        setErrMsg={setErrMsg}
      />
    </Stack>
  )
}

export default SolveSettings