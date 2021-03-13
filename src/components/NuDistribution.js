import {
  Center,
  HStack,
  Box,
  Stack,
  Input
} from "@chakra-ui/react";
import { useEffect, useRef, useState } from "react";
import * as tauri from "tauri/api/tauri";

function Nu2dDistribution({
  nu2d,
  regionShape,
  setPos,
  setNu2d,
  nuNanMean,
  w,
  h,
  setErrMsg,
}) {
  const canvas = useRef(null);
  const [vmin, setVmin] = useState(0);
  const [vmax, setVmax] = useState(0);

  useEffect(() => {
    setVmin((nuNanMean * 0.6).toFixed(2));
    setVmax((nuNanMean * 2.0).toFixed(2));
  }, [nuNanMean]);

  useEffect(() => drawNu2d(), [nu2d]);

  function drawHeader(nu) {
    const ctx = canvas.current.getContext("2d");
    ctx.fillStyle = "#282828";
    ctx.fillRect(0, 0, w, 20);
    if (!!nu) {
      ctx.fillStyle = "#cc241d";
      ctx.font = "16px serif";
      ctx.fillText(`Nu: ${nu.toFixed(2)}`, 5, 16);
    }
  }

  function drawNu2d() {
    const ctx = canvas.current.getContext("2d");
    if (!nu2d) {
      ctx.fillStyle = "#3c3836";
      ctx.fillRect(0, 20, w, h);
      ctx.fillStyle = "#fbf1c7";
      ctx.font = "30px serif";
      ctx.fillText("待求解", w / 2 - 50, h / 2 + 20);
      return;
    }

    const img = new Image();
    img.src = `data:image/png;base64,${nu2d}`;
    img.onload = () => ctx.drawImage(img, 0, 20, w, h);
  }

  async function handleClick(e) {
    const x = e.nativeEvent.offsetX - canvas.current.clientLeft;
    const y = e.nativeEvent.offsetY - canvas.current.clientTop;
    const nu = await getPointNu(x, y);
    drawHeader(nu);
  }

  function handleMouseOut() {
    drawHeader(null);
  }

  async function getPointNu(x, y) {
    const [rh, rw] = regionShape;
    const [yy, xx] = [parseInt((y - 20) / h * rh), parseInt(x / w * rw)];
    const nu = await tauri.promisified({
      cmd: "getPointNu",
      body: { UintVec: [rh - yy, xx] },
    });
    setPos([xx, yy]);
    return parseFloat(nu);
  }

  function setColorRange() {
    const min = parseFloat(vmin);
    if (isNaN(min)) {
      setErrMsg(`不合法的最小值${vmin}`);
      return;
    }
    const max = parseFloat(vmax);
    if (isNaN(max)) {
      setErrMsg(`不合法的最大值${vmax}`);
      return;
    }
    tauri.promisified({
      cmd: "setColorRange",
      body: { FloatVec: [min, max] },
    })
      .then(ok => setNu2d(ok))
      .catch(err => setErrMsg(err));
  }

  return (
    <HStack>
      <Stack spacing={0}>
        <Center
          color="#fbf1c7"
          fontSize="lg"
          fontWeight="bold"
          h="52px"
        >
          {"努塞尔数分布" + (nuNanMean ? `(ave: ${nuNanMean.toFixed(2)})` : "")}
        </Center>
        <canvas
          width={w}
          height={h + 20}
          onClick={handleClick}
          onMouseOut={handleMouseOut}
          ref={canvas}
        >
        </canvas>
        <Box h="32px"></Box>
      </Stack>
      {!!nuNanMean &&
        <Stack>
          <Box h={h - 110}></Box>
          <HStack marginX="10px">
            <Box
              w="20px"
              h="140px"
              rounded="md"
              bgGradient={"linear(\
              rgb(128,0,0), rgb(252,0,0), rgb(255,128,0), rgb(255,252,0),\
              rgb(130,255,126), rgb(0,252,255), rgb(0,128,255), rgb(0,0,255),\
              rgb(0,0,128))"}
            />
            <Stack>
              <Input
                w="70px"
                border={0}
                size="sm"
                color="#fbf1c7"
                value={vmax}
                onChange={e => setVmax(e.target.value)}
                onBlur={() => setColorRange()}
              />
              <Box h="80px"></Box>
              <Input
                w="70px"
                border={0}
                size="sm"
                color="#fbf1c7"
                value={vmin}
                onChange={e => setVmin(e.target.value)}
                onBlur={() => setColorRange()}
              />
            </Stack>
          </HStack>
        </Stack>
      }
    </HStack>
  )
}

export default Nu2dDistribution