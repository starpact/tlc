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
const D = 20;

const from = pos => pos / 2;
const into = pos => pos * 2;

function VideoCanvas({
  setFrameIndex,
  config,
  setConfig,
  setErrMsg,
}) {
  const [frame, setFrame] = useState("");
  const [showPos, setShowPos] = useState(false);
  const [innerFrameIndex, setInnerFrameIndex] = useState(0);
  const canvas = useRef(null);
  const region = useRef(null); // 区域选框
  const tcs = useRef(null); // 各热电偶
  const zoom = useRef(null); // ZOOM中的一个
  const dragTarget = useRef(null); // 可能是区域选框或某一个热电偶
  const startX = useRef(null);
  const startY = useRef(null);
  const W = from(!!config && config.video_shape[1]);
  const H = from(!!config && config.video_shape[0]);

  useEffect(() => {
    setTimeout(() => getFrame(0), 200);
  }, []);

  useEffect(() => {
    region.current = {
      x: from(config.top_left_pos[1]),
      y: from(config.top_left_pos[0]),
      w: from(config.region_shape[1]),
      h: from(config.region_shape[0]),
    };
    tcs.current = [];
    for (let i = 0; i < config.thermocouples.length; i++) {
      tcs.current.push({
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
      .catch(err => setErrMsg(err));
  }

  async function onSubmit() {
    const { x, y, w, h } = region.current;
    try {
      await tauri.promisified({
        cmd: "setRegion",
        body: { UintVec: [into(y), into(x), into(h), into(w)] },
      });
      const ok = await tauri.promisified({
        cmd: "setThermocouples",
        body: {
          Thermocouples: tcs.current.map(({ tag, x, y }) => {
            return { column_num: tag - 1, pos: [into(y), into(x)] };
          })
        }
      });
      setConfig(ok);
    } catch (err) {
      setErrMsg(err);
    }
  }

  function deleteThermocouple(targetId) {
    tcs.current = tcs.current.filter(({ id }) => id !== targetId);
    config.thermocouples = tcs.current.map(({ tag, x, y }) => {
      return {
        column_num: tag - 1,
        pos: [into(y), into(x)]
      };
    })
    setConfig(Object.assign({}, config));
  }

  function updateThermocouple(targetId, xx, yy) {
    config.thermocouples = tcs.current.map(({ id, tag, x, y }) => {
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
        const { x, y, w, h } = region.current;
        ctx.strokeRect(x, y, w, h);

        ctx.fillStyle = "#cc241d";
        ctx.font = "20px serif";
        ctx.fillText("计算区域：", 3, 20);
        ctx.fillText(`x: ${into(x)} y: ${into(y)} w: ${into(w)} h: ${into(h)}`, 5, 40);
        tcs.current.forEach(({ id, tag, x, y }) => {
          if (x < 0 || x > W || y < 0 || y > H) return;
          if (!!dragTarget.current && dragTarget.current.id === id) {
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
    if (!region.current) return;
    const [x, y] = [startX.current, startY.current];
    for (let i = 0; i < tcs.current.length; i++) {
      const tc = tcs.current[i];
      // 1.拖动热电偶
      if ((x - tc.x) ** 2 + (y - tc.y) ** 2 < RADIUS ** 2) {
        dragTarget.current = tc;
        return;
      }
    }

    const { x: rx, y: ry, w: rw, h: rh } = region.current;
    // 2.拖动区域框选
    if (x > rx + D && x < rx + rw - D && y > ry + D && y < ry + rh - D) {
      dragTarget.current = region.current;
      return;
    }
    // 3.缩放区域框选
    if (x >= rx - D && x <= rx + D && y >= ry + D && y <= ry + rh - D) {
      zoom.current = ZOOM.L;
    } else if (x >= rx + rw - D && x <= rx + rw + D && y >= ry + D && y <= ry + rh - D) {
      zoom.current = ZOOM.R;
    } else if (x >= rx + D && x <= rx + rw - D && y >= ry - D && y <= ry + D) {
      zoom.current = ZOOM.T;
    } else if (x >= rx + D && x <= rx + rw - D && y >= ry + rh - D && y <= ry + rh + D) {
      zoom.current = ZOOM.B;
    } else if (x > rx - D && x < rx + D && y > ry - D && y < ry + D) {
      zoom.current = ZOOM.TL;
    } else if (x > rx - D && x < rx + D && y > ry + rh - D && y < ry + rh + D) {
      zoom.current = ZOOM.BL;
    } else if (x > rx + rw - D && x < rx + rw + D && y > ry - D && y < ry + D) {
      zoom.current = ZOOM.TR;
    } else if (x > rx + rw - D && x < rx + rw + D && y > ry + rh - D && y < ry + rh + D) {
      zoom.current = ZOOM.BR;
    }
  }

  function handleMouseDown(e) {
    startY.current = e.nativeEvent.offsetY - canvas.current.clientTop;
    startX.current = e.nativeEvent.offsetX - canvas.current.clientLeft;
    determineTarget();
    if (e.button === 2 && !!dragTarget.current && !!dragTarget.current.tag) {
      deleteThermocouple(dragTarget.current.id);
    }
  }

  function handleMouseMove(e) {
    if (!dragTarget.current && !zoom.current) return;

    const mouseX = e.nativeEvent.offsetX - canvas.current.clientLeft;
    const mouseY = e.nativeEvent.offsetY - canvas.current.clientTop;
    const dx = mouseX - startX.current;
    const dy = mouseY - startY.current;
    startX.current = mouseX;
    startY.current = mouseY;
    if (dragTarget.current) {
      const [targetX, targetY] = [dragTarget.current.x + dx, dragTarget.current.y + dy];
      if (targetX >= 0 && targetX <= W && targetY >= 0 && targetY <= H) {
        if (!!dragTarget.current.tag
          || (targetX + dragTarget.current.w <= W && targetY + dragTarget.current.h <= H)) {
          dragTarget.current.x += dx;
          dragTarget.current.y += dy;
        }
      }
    } else {
      switch (zoom.current) {
        case ZOOM.L:
          {
            const [x, w] = [region.current.x + dx, region.current.w - dx];
            if (x >= 0 && w >= 4 * D) {
              [region.current.x, region.current.w] = [x, w];
            }
          }
          break;
        case ZOOM.R:
          {
            const w = region.current.w + dx;
            if (w >= 4 * D && region.current.x + w <= W) {
              region.current.w = w;
            }
          }
          break;
        case ZOOM.T:
          {
            const [y, h] = [region.current.y + dy, region.current.h - dy];
            if (y >= 0 && h >= 4 * D) {
              [region.current.y, region.current.h] = [y, h];
            }
          }
          break;
        case ZOOM.B:
          {
            const h = region.current.h + dy;
            if (h >= 4 * D && region.current.y + h <= H) {
              region.current.h = h;
            }
          }
          break;
        case ZOOM.TL:
          {
            const [x, y, w, h] = [
              region.current.x + dx,
              region.current.y + dy,
              region.current.w - dx,
              region.current.h - dy
            ];
            if (x >= 0 && y >= 0 && w > 4 * D && h >= 4 * D) {
              [region.current.x, region.current.y, region.current.w, region.current.h]
                = [x, y, w, h];
            }
          }
          break;
        case ZOOM.BL:
          {
            const [x, w, h] = [region.current.x + dx, region.current.w - dx, region.current.h + dy];
            if (x >= 0 && w >= 4 * D && h >= 4 * D && region.current.y + h <= H) {
              [region.current.x, region.current.w, region.current.h] = [x, w, h];
            }
          }
          break;
        case ZOOM.TR:
          {
            const [y, w, h] = [region.current.y + dy, region.current.w + dx, region.current.h - dy];
            if (y >= 0 && w >= 4 * D && region.current.x + w <= W && h >= 4 * D) {
              [region.current.y, region.current.w, region.current.h] = [y, w, h];
            }
          }
          break;
        case ZOOM.BR:
          {
            const [w, h] = [region.current.w + dx, region.current.h + dy];
            if (w >= 4 * D && region.current.x + w <= W && h >= 4 * D && region.current.y + h <= H) {
              [region.current.w, region.current.h] = [w, h];
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
    dragTarget.current = null;
    zoom.current = null;
  }

  // 鼠标移出canvas时提交更新config数据
  // 在拖动时不会触发重新渲染的前提下，保证其他组件能拿到ref里的最新数据
  // 防止因为忘记提交而造成前后端数据不一致
  function handleMouseOut() {
    onSubmit();
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
        <ITag text={innerFrameIndex + 1} w="60px" />
        <Box w={3} />
        <Slider
          defaultValue={0}
          min={0}
          max={config.total_frames - 1}
          onChange={v => {
            const vv = parseInt(v);
            getFrame(vv);
            setInnerFrameIndex(vv);
          }}
          onChangeEnd={v => {
            const vv = parseInt(v);
            setFrameIndex(vv);
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
          checked={showPos}
          onChange={e => setShowPos(e.target.checked)} >
          显示计算区域和热电偶
        </Checkbox>
        <Box w="7px"></Box>
        <Box w="7px"></Box>
        {showPos && tcs.current && tcs.current.map(({ id, tag, x, y }) =>
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
    </Stack >
  );
}

export default VideoCanvas;