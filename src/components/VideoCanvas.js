import React, { useRef, useEffect, useState } from 'react';
import {
  Checkbox,
  HStack,
  Slider,
  SliderTrack,
  SliderFilledTrack,
  SliderThumb,
  Box,
  Text,
  Stack,
} from "@chakra-ui/react";
import * as tauri from 'tauri/api/tauri';
import IButton from './Button';

// 矩形的8个缩放方向
// 辣鸡js连枚举都没有
const ZOOM = {
  L: 995, // left
  R: "大小周",
  T: 996, // top
  B: "007", // bottom
  TL: "我", // top left
  BL: "爱",
  TR: "加",
  BR: "班"
}

// 热电偶位置图标半径
const RADIUS = 10;

// 区域选框触发缩放操作的宽度
const D = 10;

function VideoCanvas({
  frameIndex,
  setFrameIndex,
  config,
  setConfig,
  setErrMsg,
}) {
  const [frame, setFrame] = useState("");
  const [showPos, setShowPos] = useState(true);
  const canvas = useRef();
  const W = !!config && config.video_shape[1] / 2;
  const H = !!config && config.video_shape[0] / 2;
  let ctx = null;

  let region = null; // 区域选框
  let tcs = []; // 各热电偶
  let zoom = null; // ZOOM中的一个
  let dragTarget = null; // 可能是区域选框或某一个热电偶
  let startX = null;
  let startY = null;

  useEffect(() => !!config && getFrame(0), []);

  useEffect(() => {
    if (!config) return;
    region = {
      x: config.top_left_pos[1] / 2,
      y: config.top_left_pos[0] / 2,
      w: config.region_shape[1] / 2,
      h: config.region_shape[0] / 2,
    };
    for (let i = 0; i < config.thermocouple_pos.length; i++) {
      tcs.push({
        tag: config.temp_column_num[i],
        x: config.thermocouple_pos[i][1] / 2,
        y: config.thermocouple_pos[i][0] / 2,
      });
    }
    ctx = canvas.current.getContext("2d");
    draw();
  }, [config, frame, showPos]);

  function getFrame(frame_index) {
    tauri.promisified({
      cmd: "getFrame",
      body: frame_index,
    })
      .then(ok => setFrame(ok))
      .catch(err => setErrMsg(err));
  }

  function onSubmit() {
    const { x, y, w, h } = region;
    tauri.promisified({
      cmd: "setRegion",
      body: [y * 2, x * 2, h * 2, w * 2],
    })
      .catch(err => { setErrMsg(err); return; });

    tauri.promisified({
      cmd: "setTempColumnNum",
      body: tcs.map(({ tag }) => tag),
    })
      .catch(err => { setErrMsg(err); return; });

    tauri.promisified({
      cmd: "setThermocouplePos",
      body: tcs.map(({ x, y }) => [y * 2, x * 2]),
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function draw() {
    const img = new Image();
    img.src = `data: image/jpeg;base64,${frame}`;
    img.onload = () => {
      ctx.drawImage(img, 0, 0, W, H);

      if (showPos) {
        ctx.strokeStyle = "#cc241d";
        const { x, y, w, h } = region;
        ctx.strokeRect(x, y, w, h);

        ctx.fillStyle = "#cc241d";
        ctx.font = "20px serif";
        ctx.fillText("计算区域：", 3, 20);
        ctx.fillText(`x: ${x * 2} y: ${y * 2} w: ${w * 2} h: ${h * 2}`, 5, 40);
        tcs.forEach(({ tag, x, y }) => {
          if (x < 0 || x > W || y < 0 || y > H) return;
          if (!!dragTarget && dragTarget.tag === tag) {
            ctx.font = "20px serif";
            ctx.fillText(`${tag}号热电偶：`, 3, 65);
            ctx.fillText(`x: ${x * 2} y: ${y * 2}`, 5, 85);
          }
          ctx.font = "15px serif";
          if (tag < 10) {
            ctx.fillText(tag, x - 4, y + 5);
          } else {
            ctx.fillText(tag, x - 7, y + 5);
          }
          ctx.strokeStyle = "#cc241d";
          ctx.beginPath();
          ctx.arc(x, y, RADIUS, 0, Math.PI * 2);
          ctx.stroke();
        });
      }
    }
  }

  function determineTarget() {
    const [x, y] = [startX, startY];
    for (let i = 0; i < tcs.length; i++) {
      // 1.拖动热电偶
      const tc = tcs[i];
      if ((x - tc.x) ** 2 + (y - tc.y) ** 2 < RADIUS ** 2) {
        dragTarget = tc;
        return;
      }
    }

    const { x: rx, y: ry, w: rw, h: rh } = region;
    // 2.拖动区域框选
    if (x > rx + D && x < rx + rw - D && y > ry + D && y < ry + rh - D) {
      dragTarget = region;
      return;
    }
    // 3.缩放区域框选
    if (x >= rx - D && x <= rx + D && y >= ry + D && y <= ry + rh - D) {
      zoom = ZOOM.L;
    } else if (x >= rx + rw - D && x <= rx + rw + D && y >= ry + D && y <= ry + rh - D) {
      zoom = ZOOM.R;
    } else if (x >= rx + D && x <= rx + rw - D && y >= ry - D && y <= ry + D) {
      zoom = ZOOM.T;
    } else if (x >= rx + D && x <= rx + rw - D && y >= ry + rh - D && y <= ry + rh + D) {
      zoom = ZOOM.B;
    } else if (x > rx - D && x < rx + D && y > ry - D && y < ry + D) {
      zoom = ZOOM.TL;
    } else if (x > rx - D && x < rx + D && y > ry + rh - D && y < ry + rh + D) {
      zoom = ZOOM.BL;
    } else if (x > rx + rw - D && x < rx + rw + D && y > ry - D && y < ry + D) {
      zoom = ZOOM.TR;
    } else if (x > rx + rw - D && x < rx + rw + D && y > ry + rh - D && y < ry + rh + D) {
      zoom = ZOOM.BR;
    }
  }

  function handleMouseDown(e) {
    startX = e.nativeEvent.offsetX - canvas.current.clientLeft;
    startY = e.nativeEvent.offsetY - canvas.current.clientTop;
    determineTarget();
  }

  function handleMouseMove(e) {
    if (!dragTarget && !zoom) return;

    const mouseX = e.nativeEvent.offsetX - canvas.current.clientLeft;
    const mouseY = e.nativeEvent.offsetY - canvas.current.clientTop;
    const dx = mouseX - startX;
    const dy = mouseY - startY;
    startX = mouseX;
    startY = mouseY;
    if (dragTarget) {
      const [targetX, targetY] = [dragTarget.x + dx, dragTarget.y + dy];
      if (targetX >= 0 && targetX <= W && targetY >= 0 && targetY <= H) {
        if (!!dragTarget.tag
          || (targetX + dragTarget.w <= W && targetY + dragTarget.h <= H)) {
          dragTarget.x += dx;
          dragTarget.y += dy;
        }
      }
    } else {
      switch (zoom) {
        case ZOOM.L:
          {
            const [x, w] = [region.x + dx, region.w - dx];
            if (x >= 0 && w >= 4 * D) {
              [region.x, region.w] = [x, w];
            }
          }
          break;
        case ZOOM.R:
          {
            const w = region.w + dx;
            if (w >= 4 * D && region.x + w <= W) {
              region.w = w;
            }
          }
          break;
        case ZOOM.T:
          {
            const [y, h] = [region.y + dy, region.h - dy];
            if (y >= 0 && h >= 4 * D) {
              [region.y, region.h] = [y, h];
            }
          }
          break;
        case ZOOM.B:
          {
            const h = region.h + dy;
            if (h >= 4 * D && region.y + h <= H) {
              region.h = h;
            }
          }
          break;
        case ZOOM.TL:
          {
            const [x, y, w, h] = [region.x + dx, region.y + dy, region.w - dx, region.h - dy];
            if (x >= 0 && y >= 0 && w > 4 * D && h >= 4 * D) {
              [region.x, region.y, region.w, region.h] = [x, y, w, h];
            }
          }
          break;
        case ZOOM.BL:
          {
            const [x, w, h] = [region.x + dx, region.w - dx, region.h + dy];
            if (x >= 0 && w >= 4 * D && h >= 4 * D && region.y + h <= H) {
              [region.x, region.w, region.h] = [x, w, h];
            }
          }
          break;
        case ZOOM.TR:
          {
            const [y, w, h] = [region.y + dy, region.w + dx, region.h - dy];
            if (y >= 0 && w >= 4 * D && region.x + w <= W && h >= 4 * D) {
              [region.y, region.w, region.h] = [y, w, h];
            }
          }
          break;
        case ZOOM.BR:
          {
            const [w, h] = [region.w + dx, region.h + dy];
            if (w > 4 * D && region.x + w <= W && h > 4 * D && region.y + h <= H) {
              [region.w, region.h] = [w, h];
            }
          }
          break;
        default:
          return;
      }
    }
    draw();
  }

  function handleMouseUp() {
    dragTarget = null;
    zoom = null;
  }

  function handleMouseOut() {
    handleMouseUp();
  }

  return (
    <HStack>
      <Stack>
        <canvas
          width={W}
          height={H}
          ondragstart="return false"
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onMouseOut={handleMouseOut}
          ref={canvas}
        >
        </canvas>
        <HStack>
          <Box
            textAlign="center"
            rounded="md"
            w="60px"
            bgColor="#98971a"
            marginLeft="1"
            marginRight="3"
          >
            <Text color="#32302f" fontWeight="bold">
              {frameIndex + 1}
            </Text>
          </Box>
          <Slider
            defaultValue={0}
            min={0}
            max={config.total_frames - 1}
            onChange={v => {
              const vv = parseInt(v);
              setFrameIndex(vv);
              getFrame(vv);
            }}
          >
            <SliderTrack bgColor="#665c54">
              <SliderFilledTrack bgColor="#98971a" />
            </SliderTrack>
            <SliderThumb bgColor="#928374" />
          </Slider>
          <Box w="10px"></Box>
        </HStack>
        <HStack h="25px">
          <Box w="7px"></Box>
          <Checkbox
            size="lg"
            colorScheme="teal"
            color="#98971a"
            defaultChecked
            checked={showPos}
            onChange={e => setShowPos(e.target.checked)}
          >
            显示计算区域和热电偶位置
          </Checkbox>
          {showPos && <IButton text="提交" onClick={onSubmit} size="sm" />}
        </HStack>
      </Stack>
    </HStack>
  );
}

export default VideoCanvas;