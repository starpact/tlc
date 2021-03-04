import { Stack } from "@chakra-ui/react";
import * as tauri from 'tauri/api/tauri';

import IInput from "../components/Input";

import Regulator from "../components/Regulator";
import SelectFilter from "../components/SelectFilter";
import SelectInterp from "../components/SelectInterp";
import SelectIteration from "../components/SelectIteration";

function SolveSettings({ config, setConfig, awsl }) {

  function setPeakTemp(peakTemp) {
    if (!peakTemp) return;
    if (Math.abs(peakTemp - config.peak_temp) < 1e-5) return;
    tauri.promisified({
      cmd: "setPeakTemp",
      body: { Float: peakTemp },
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  function setSolidThermalConductivity(solidThermalConductivity) {
    if (!solidThermalConductivity) return;
    if (Math.abs(solidThermalConductivity - config.solid_thermal_conductivity) < 1e-5) return;
    tauri.promisified({
      cmd: "setSolidThermalConductivity",
      body: { Float: solidThermalConductivity },
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  function setSolidThermalDiffusivity(solidThermalDiffusivity) {
    if (!solidThermalDiffusivity) return;
    tauri.promisified({
      cmd: "setSolidThermalDiffusivity",
      body: { Float: solidThermalDiffusivity },
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  function setAirThermalConductivity(airThermalConductivity) {
    if (!airThermalConductivity) return;
    if (Math.abs(airThermalConductivity - config.air_thermal_conductivity) < 1e-5) return;
    tauri.promisified({
      cmd: "setAirThermalConductivity",
      body: { Float: airThermalConductivity },
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  function setCharacteristicLength(characteristicLength) {
    if (!characteristicLength) return;
    if (Math.abs(characteristicLength - config.characteristic_length) < 1e-5) return;
    tauri.promisified({
      cmd: "setCharacteristicLength",
      body: { Float: characteristicLength },
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  function setRegulator(regulator) {
    tauri.promisified({
      cmd: "setRegulator",
      body: { FloatVec: regulator },
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  function setFilterMethod(filterMethod) {
    tauri.promisified({
      cmd: "setFilterMethod",
      body: { Filter: filterMethod },
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  function setInterpMethod(interpMethod) {
    tauri.promisified({
      cmd: "setInterpMethod",
      body: { Interp: interpMethod },
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  function setIterationMethod(iterationMethod) {
    tauri.promisified({
      cmd: "setIterationMethod",
      body: { Iteration: iterationMethod },
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err))
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
        awsl={awsl}
      />
      <SelectInterp
        value={config.interp_method}
        onSubmit={setInterpMethod}
        awsl={awsl}
      />
      <SelectIteration
        value={config.iteration_method}
        onSubmit={setIterationMethod}
        awsl={awsl}
      />
    </Stack>
  )
}

export default SolveSettings