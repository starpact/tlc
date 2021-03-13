import {
  Box,
  Grid,
  GridItem,
  HStack,
} from "@chakra-ui/react";
import * as tauri from "tauri/api/tauri";

import IInput from "../components/Input";

import Regulator from "../components/Regulator";
import SelectFilter from "../components/SelectFilter";
import SelectInterp from "../components/SelectInterp";
import SelectIteration from "../components/SelectIteration";
import InterpDistribution from "../components/InterpDistribution";
import { useState, useEffect } from "react";
import GreenHistoryLine from "../components/GreenHistoryLine";
import NuDistribution from "../components/NuDistribution";

function SolveSettings({ config, setConfig, setErrMsg }) {
  const [interp, setInterp] = useState(null);
  const [currentFrame, setCurrentFrame] = useState(parseInt(config.frame_num / 2));
  const [showRegulator, setShowRegulator] = useState(false);
  const [nu2d, setNu2d] = useState(null);
  const [nuNanMean, setNuNanMean] = useState(null);
  const [pos, setPos] = useState(!!config.region_shape && [
    parseInt(config.region_shape[1] / 2),
    parseInt(config.region_shape[0] / 2),
  ]);
  const [history, setHistory] = useState(null);
  let [H, W] = config.region_shape;
  H = 670 * H / W;
  W = 670;

  // 先主动触发后端对热电偶的排序
  useEffect(() => setInterpMethod(config.interp_method), []);

  useEffect(() => {
    setInterp(null);
    tauri.promisified({
      cmd: "getInterpSingleFrame",
      body: { Uint: currentFrame - 1 },
    })
      .then(ok => setInterp(ok))
      .catch(err => setErrMsg(err));
  }, [config.interp_method, currentFrame]);

  useEffect(() => {
    tauri.promisified({
      cmd: "getGreenHistory",
      body: { Uint: pos[1] * config.region_shape[1] + pos[0] },
    })
      .then(ok => setHistory(ok))
      .catch(err => setErrMsg(err));
  }, [config.filter_method, config.region_shape, pos]);

  function setPeakTemp(peakTemp) {
    if (isNaN(peakTemp)) {
      setErrMsg("不合法的峰值温度");
      return;
    }
    if (Math.abs(peakTemp - config.peak_temp) < 1e-5) return;
    tauri.promisified({
      cmd: "setPeakTemp",
      body: { Float: peakTemp },
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setSolidThermalConductivity(solidThermalConductivity) {
    if (isNaN(solidThermalConductivity)) {
      setErrMsg("不合法的固体导热系数");
      return;
    }
    if (Math.abs(solidThermalConductivity - config.solid_thermal_conductivity) < 1e-5) return;
    tauri.promisified({
      cmd: "setSolidThermalConductivity",
      body: { Float: solidThermalConductivity },
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setSolidThermalDiffusivity(solidThermalDiffusivity) {
    if (isNaN(solidThermalDiffusivity)) {
      setErrMsg("不合法的固体热扩散系数");
      return;
    }
    tauri.promisified({
      cmd: "setSolidThermalDiffusivity",
      body: { Float: solidThermalDiffusivity },
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setAirThermalConductivity(airThermalConductivity) {
    if (isNaN(airThermalConductivity)) {
      setErrMsg("不合法的空气导热系数");
      return;
    }
    if (Math.abs(airThermalConductivity - config.air_thermal_conductivity) < 1e-5) return;
    tauri.promisified({
      cmd: "setAirThermalConductivity",
      body: { Float: airThermalConductivity },
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setCharacteristicLength(characteristicLength) {
    if (isNaN(characteristicLength)) {
      setErrMsg("不合法的特征长度");
      return;
    }
    if (Math.abs(characteristicLength - config.characteristic_length) < 1e-5) return;
    tauri.promisified({
      cmd: "setCharacteristicLength",
      body: { Float: characteristicLength },
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setRegulator(regulator) {
    tauri.promisified({
      cmd: "setRegulator",
      body: { FloatVec: regulator },
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setFilterMethod(filterMethod) {
    tauri.promisified({
      cmd: "setFilterMethod",
      body: { Filter: filterMethod },
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setInterpMethod(interpMethod) {
    tauri.promisified({
      cmd: "setInterpMethod",
      body: { Interp: interpMethod },
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setIterationMethod(iterationMethod) {
    setNuNanMean(null);
    setNu2d(null);
    tauri.promisified({
      cmd: "setIterationMethod",
      body: { Iteration: iterationMethod },
    })
      .then(ok => {
        config.iteration_method = iterationMethod;
        setConfig(Object.assign({}, config));
        setNuNanMean(ok[1]);
        setNu2d(ok[0]);
      })
      .catch(err => setErrMsg(err))
  }

  return (
    <Box>
      {config !== "" &&
        <Grid
          templateRows="repeat(5, 1fr)"
          templateColumns="repeat(9, 1fr)"
          gap={2}
          marginX="25px"
        >
          <GridItem colSpan={3}>
            <IInput
              leftTag="峰值温度"
              value={!!config.peak_temp ? config.peak_temp.toPrecision(4) : ""}
              onBlur={v => setPeakTemp(parseFloat(v))}
              mutable
              rightTag="°C"
            />
          </GridItem>
          <GridItem colSpan={3} rowSpan={1}>
            <IInput
              leftTag="特征长度"
              value={!!config.characteristic_length
                ? config.characteristic_length.toFixed(4) : ""}
              onBlur={v => setCharacteristicLength(parseFloat(v))}
              mutable
              rightTag="m"
            />
          </GridItem>
          <GridItem colSpan={3} rowSpan={1}>
            <IInput
              leftTag="当前插值帧数"
              hover="从同步后的起始帧数开始计数"
              value={currentFrame}
              onBlur={v => {
                if (v === "happiness") {
                  setShowRegulator(true);
                  return;
                }
                setShowRegulator(false);
                const vv = parseInt(v);
                if (isNaN(vv) || vv <= 0 || vv > config.frame_num) {
                  setErrMsg(`不合法的帧数：${v}`);
                  return;
                }
                setCurrentFrame(vv);
              }}
              mutable
              rightTag={`(1, ${config.frame_num})`}
            />
          </GridItem>
          <GridItem colSpan={3}>
            <IInput
              leftTag="固体导热系数"
              value={!!config.solid_thermal_conductivity
                ? config.solid_thermal_conductivity.toPrecision(3) : ""}
              onBlur={v => setSolidThermalConductivity(parseFloat(v))}
              mutable
              rightTag="W/(m·K)"
            />
          </GridItem>
          <GridItem colSpan={3}>
            <IInput
              leftTag="固体热扩散系数"
              value={!!config.solid_thermal_diffusivity
                ? config.solid_thermal_diffusivity.toPrecision(4) : ""}
              rightTag="m2/s"
              onBlur={v => setSolidThermalDiffusivity(parseFloat(v))}
              mutable
            />
          </GridItem>
          <GridItem colSpan={3} rowSpan={1}>
            <IInput
              leftTag="气体导热系数"
              value={!!config.air_thermal_conductivity
                ? config.air_thermal_conductivity.toPrecision(3) : ""}
              onBlur={v => setAirThermalConductivity(parseFloat(v))}
              mutable
              rightTag="W/(m·K)"
            />
          </GridItem>
          <GridItem colSpan={4} rowSpan={1}>
            <SelectInterp
              value={config.interp_method}
              onSubmit={setInterpMethod}
              setErrMsg={setErrMsg}
            />
          </GridItem>
          <GridItem colSpan={4} rowSpan={1}>
            <SelectFilter
              value={config.filter_method}
              onSubmit={setFilterMethod}
              setErrMsg={setErrMsg}
            />
          </GridItem>
          <GridItem colSpan={4} rowSpan={1}>
            <SelectIteration
              value={config.iteration_method}
              onSubmit={setIterationMethod}
              setErrMsg={setErrMsg}
            />
          </GridItem>
          <GridItem rowStart={3} colStart={5} colSpan={5} rowSpan={3}>
            {showRegulator ?
              <Regulator
                regulator={config.regulator}
                onSubmit={setRegulator}
              />
              : !!history &&
              <GreenHistoryLine
                history={history}
                pos={pos}
              />}
          </GridItem>
        </Grid>}
      <HStack marginTop={H < 360 ? 180 - H / 2 : 0}>
        {!!interp &&
          <InterpDistribution
            interp={interp}
            setPos={setPos}
            w={W}
            h={H}
          />}
        <Box w="20px" />
        <NuDistribution
          nu2d={nu2d}
          setNu2d={setNu2d}
          regionShape={config.region_shape}
          setPos={setPos}
          nuNanMean={nuNanMean}
          w={W}
          h={H}
          setErrMsg={setErrMsg}
        />
      </HStack>
    </Box>
  )
}

export default SolveSettings