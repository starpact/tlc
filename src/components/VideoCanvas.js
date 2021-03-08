import React, { useRef, useEffect, useState } from "react";
import {
  Checkbox,
  HStack,
  Slider,
  SliderTrack,
  SliderFilledTrack,
  SliderThumb,
  Box,
  Stack,
  Popover,
  PopoverTrigger,
  PopoverContent,
  PopoverHeader,
  PopoverBody,
  PopoverArrow,
  PopoverCloseButton,
  Tooltip,
  Text,
} from "@chakra-ui/react";
import * as tauri from "tauri/api/tauri";
import IButton from "./Button";
import ITag from "./Tag";
import IPopover from "./Popover";

// 矩形的8个缩放方向
// js没有枚举...
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

const from = pos => pos / 2;
const into = pos => pos * 2;

function VideoCanvas({
  frameIndex,
  setFrameIndex,
  config,
  setConfig,
  awsl,
}) {
  const [frame, setFrame] = useState("");
  const [showPos, setShowPos] = useState(true);
  const canvas = useRef(null);
  const r = useRef(null);
  const W = from(!!config && config.video_shape[1]);
  const H = from(!!config && config.video_shape[0]);

  useEffect(() => {
    r.current = {
      region: null,// 区域选框
      tcs: null, // 各热电偶
      zoom: null, // ZOOM中的一个
      dragTarget: null, // 可能是区域选框或某一个热电偶
      startX: null,
      startY: null,
    };
    setTimeout(() => getFrame(0), 200);
  }, []);

  useEffect(() => {
    r.current.region = {
      x: from(config.top_left_pos[1]),
      y: from(config.top_left_pos[0]),
      w: from(config.region_shape[1]),
      h: from(config.region_shape[0]),
    };
    r.current.tcs = [];
    for (let i = 0; i < config.thermocouples.length; i++) {
      r.current.tcs.push({
        id: i,
        tag: config.thermocouples[i].column_num + 1,
        x: from(config.thermocouples[i].pos[1]),
        y: from(config.thermocouples[i].pos[0]),
      });
    }
    draw();
  }, [config, frame, showPos]);

  function getFrame(frame_index) {
    tauri.promisified({
      cmd: "getFrame",
      body: { Uint: frame_index },
    })
      .then(ok => setFrame(ok))
      .catch(err => awsl(err));
  }

  function onSubmit() {
    const { x, y, w, h } = r.current.region;
    tauri.promisified({
      cmd: "setRegion",
      body: { UintVec: [into(y), into(x), into(h), into(w)] },
    })
      .catch(err => { awsl(err); return; });

    tauri.promisified({
      cmd: "setThermocouples",
      body: {
        Thermocouples: r.current.tcs.map(({ tag, x, y }) => {
          return {
            column_num: tag - 1,
            pos: [into(y), into(x)]
          };
        })
      }
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  function deleteThermocouple(targetId) {
    r.current.tcs = r.current.tcs.filter(({ id }) => id !== targetId);
    config.thermocouples = r.current.tcs.map(({ tag, x, y }) => {
      return {
        column_num: tag - 1,
        pos: [into(y), into(x)]
      };
    })
    setConfig(Object.assign({}, config));
  }

  function updateThermocouple(targetId, xx, yy) {
    config.thermocouples = r.current.tcs.map(({ id, tag, x, y }) => {
      if (id === targetId) {
        return {
          column_num: tag - 1,
          pos: [yy, xx]
        };
      } else {
        return {
          column_num: tag - 1,
          pos: [into(y), into(x)]
        };
      }
    });
    setConfig(Object.assign({}, config));
  }

  function draw() {
    const ctx = canvas.current.getContext("2d");
    if (frame === "") {
      ctx.fillStyle = "#3c3836";
      ctx.fillRect(0, 0, W, H);
      ctx.fillStyle = "#fbf1c7";
      ctx.font = "30px serif";
      ctx.fillText("请选择视频", W / 2 - 80, H / 2);
      return;
    }

    const img = new Image();
    img.src = `data: image/jpeg;base64,${frame}`;
    img.onload = () => {
      ctx.drawImage(img, 0, 0, W, H);

      if (showPos) {
        ctx.strokeStyle = "#cc241d";
        const { x, y, w, h } = r.current.region;
        ctx.strokeRect(x, y, w, h);

        ctx.fillStyle = "#cc241d";
        ctx.font = "20px serif";
        ctx.fillText("计算区域：", 3, 20);
        ctx.fillText(`x: ${into(x)} y: ${into(y)} w: ${into(w)} h: ${into(h)}`, 5, 40);
        r.current.tcs.forEach(({ id, tag, x, y }) => {
          if (x < 0 || x > W || y < 0 || y > H) return;
          if (!!r.current.dragTarget && r.current.dragTarget.id === id) {
            ctx.font = "20px serif";
            ctx.fillText(`${tag}号热电偶：`, 3, 65);
            ctx.fillText(`x: ${into(x)} y: ${into(y)}`, 5, 85);
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
    if (!r.current.region) return;
    const [x, y] = [r.current.startX, r.current.startY];
    for (let i = 0; i < r.current.tcs.length; i++) {
      const tc = r.current.tcs[i];
      // 1.拖动热电偶
      if ((x - tc.x) ** 2 + (y - tc.y) ** 2 < RADIUS ** 2) {
        r.current.dragTarget = tc;
        return;
      }
    }

    const { x: rx, y: ry, w: rw, h: rh } = r.current.region;
    // 2.拖动区域框选
    if (x > rx + D && x < rx + rw - D && y > ry + D && y < ry + rh - D) {
      r.current.dragTarget = r.current.region;
      return;
    }
    // 3.缩放区域框选
    if (x >= rx - D && x <= rx + D && y >= ry + D && y <= ry + rh - D) {
      r.current.zoom = ZOOM.L;
    } else if (x >= rx + rw - D && x <= rx + rw + D && y >= ry + D && y <= ry + rh - D) {
      r.current.zoom = ZOOM.R;
    } else if (x >= rx + D && x <= rx + rw - D && y >= ry - D && y <= ry + D) {
      r.current.zoom = ZOOM.T;
    } else if (x >= rx + D && x <= rx + rw - D && y >= ry + rh - D && y <= ry + rh + D) {
      r.current.zoom = ZOOM.B;
    } else if (x > rx - D && x < rx + D && y > ry - D && y < ry + D) {
      r.current.zoom = ZOOM.TL;
    } else if (x > rx - D && x < rx + D && y > ry + rh - D && y < ry + rh + D) {
      r.current.zoom = ZOOM.BL;
    } else if (x > rx + rw - D && x < rx + rw + D && y > ry - D && y < ry + D) {
      r.current.zoom = ZOOM.TR;
    } else if (x > rx + rw - D && x < rx + rw + D && y > ry + rh - D && y < ry + rh + D) {
      r.current.zoom = ZOOM.BR;
    }
  }

  function handleMouseDown(e) {
    r.current.startY = e.nativeEvent.offsetY - canvas.current.clientTop;
    r.current.startX = e.nativeEvent.offsetX - canvas.current.clientLeft;
    determineTarget();
    if (e.button === 2 && !!r.current.dragTarget && !!r.current.dragTarget.tag) {
      deleteThermocouple(r.current.dragTarget.id);
    }
  }

  function handleMouseMove(e) {
    if (!r.current.dragTarget && !r.current.zoom) return;

    const mouseX = e.nativeEvent.offsetX - canvas.current.clientLeft;
    const mouseY = e.nativeEvent.offsetY - canvas.current.clientTop;
    const dx = mouseX - r.current.startX;
    const dy = mouseY - r.current.startY;
    r.current.startX = mouseX;
    r.current.startY = mouseY;
    if (r.current.dragTarget) {
      const [targetX, targetY] = [r.current.dragTarget.x + dx, r.current.dragTarget.y + dy];
      if (targetX >= 0 && targetX <= W && targetY >= 0 && targetY <= H) {
        if (!!r.current.dragTarget.tag
          || (targetX + r.current.dragTarget.w <= W && targetY + r.current.dragTarget.h <= H)) {
          r.current.dragTarget.x += dx;
          r.current.dragTarget.y += dy;
        }
      }
    } else {
      switch (r.current.zoom) {
        case ZOOM.L:
          {
            const [x, w] = [r.current.region.x + dx, r.current.region.w - dx];
            if (x >= 0 && w >= 4 * D) {
              [r.current.region.x, r.current.region.w] = [x, w];
            }
          }
          break;
        case ZOOM.R:
          {
            const w = r.current.region.w + dx;
            if (w >= 4 * D && r.current.region.x + w <= W) {
              r.current.region.w = w;
            }
          }
          break;
        case ZOOM.T:
          {
            const [y, h] = [r.current.region.y + dy, r.current.region.h - dy];
            if (y >= 0 && h >= 4 * D) {
              [r.current.region.y, r.current.region.h] = [y, h];
            }
          }
          break;
        case ZOOM.B:
          {
            const h = r.current.region.h + dy;
            if (h >= 4 * D && r.current.region.y + h <= H) {
              r.current.region.h = h;
            }
          }
          break;
        case ZOOM.TL:
          {
            const [x, y, w, h] = [
              r.current.region.x + dx,
              r.current.region.y + dy,
              r.current.region.w - dx,
              r.current.region.h - dy
            ];
            if (x >= 0 && y >= 0 && w > 4 * D && h >= 4 * D) {
              [r.current.region.x, r.current.region.y, r.current.region.w, r.current.region.h]
                = [x, y, w, h];
            }
          }
          break;
        case ZOOM.BL:
          {
            const [x, w, h] = [r.current.region.x + dx, r.current.region.w - dx, r.current.region.h + dy];
            if (x >= 0 && w >= 4 * D && h >= 4 * D && r.current.region.y + h <= H) {
              [r.current.region.x, r.current.region.w, r.current.region.h] = [x, w, h];
            }
          }
          break;
        case ZOOM.TR:
          {
            const [y, w, h] = [r.current.region.y + dy, r.current.region.w + dx, r.current.region.h - dy];
            if (y >= 0 && w >= 4 * D && r.current.region.x + w <= W && h >= 4 * D) {
              [r.current.region.y, r.current.region.w, r.current.region.h] = [y, w, h];
            }
          }
          break;
        case ZOOM.BR:
          {
            const [w, h] = [r.current.region.w + dx, r.current.region.h + dy];
            if (w >= 4 * D && r.current.region.x + w <= W && h >= 4 * D && r.current.region.y + h <= H) {
              [r.current.region.w, r.current.region.h] = [w, h];
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
    r.current.dragTarget = null;
    r.current.zoom = null;
  }

  // 鼠标移出canvas时更新config数据
  // 保证在拖动时不会触发重新渲染的前提下，其他组件能拿到ref里的最新数据
  function handleMouseOut() {
    const { x, y, w, h } = r.current.region;
    config.top_left_pos = [into(y), into(x)];
    config.region_shape = [into(h), into(w)];
    config.thermocouples = r.current.tcs.map(({ tag, x, y }) => {
      return {
        column_num: tag - 1,
        pos: [into(y), into(x)]
      };
    })
    setConfig(Object.assign({}, config));
  }

  return (
    <Stack>
      <canvas
        width={W}
        height={H}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseOut={handleMouseOut}
        ref={canvas}
      >
      </canvas>
      <HStack>
        <ITag text={frameIndex + 1} w="60px" />
        <Box w={3} />
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
      </HStack>
      <HStack h="25px">
        <Checkbox
          size="lg"
          colorScheme="teal"
          color="#98971a"
          defaultChecked
          checked={showPos}
          onChange={e => setShowPos(e.target.checked)} >
          显示计算区域和热电偶
        </Checkbox>
        <Box w="7px"></Box>
        {showPos && <IButton text="提交" onClick={onSubmit} size="sm" />}
        <Box w="7px"></Box>
        {showPos && !!r && !!r.current && r.current.tcs.map(({ id, tag, x, y }) =>
          <Popover>
            <PopoverTrigger>
              <Box>
                <Tooltip label={`x: ${into(x)} y: ${into(y)}`}>
                  <Text
                    rounded="full"
                    marginLeft="2px"
                    marginRight="2px"
                    w="30px"
                    border="solid"
                    onMouseDown={e => { if (e.button === 2) deleteThermocouple(id); }}
                    color="#98971a"
                    fontWeight="bold"
                    align="center"
                  >
                    {`${tag}`}
                  </Text>
                </Tooltip>
              </Box>
            </PopoverTrigger>
            <PopoverContent bgColor="#282828" color="#d79921" borderColor="#282828">
              <PopoverArrow bgColor="#282828" />
              <PopoverCloseButton color="#fbf1c7" />
              <PopoverHeader fontSize="lg" border="0">
                手动设置镜头范围外的热电偶位置
                </PopoverHeader>
              <PopoverBody>
                <IPopover
                  id={id} x={into(x)} y={into(y)}
                  updateThermocouple={updateThermocouple}
                  into={into}
                />
              </PopoverBody>
            </PopoverContent>
          </Popover>
        )}
      </HStack>
    </Stack>
  );
}

export default VideoCanvas;