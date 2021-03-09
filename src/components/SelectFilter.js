import { Box, HStack, Select } from "@chakra-ui/react";
import { useState } from "react";
import IButton from "./Button";
import IInput from "./Input";

function SelectFilter({ value, onSubmit, setErrMsg }) {
  const [innerValue, setInnerValue] = useState(value);

  const onSelectChange = v => {
    switch (v.target.value) {
      case "Median": setInnerValue({ Median: 20 }); break;
      case "Wavelet": setInnerValue({ Wavelet: 0.5 }); break;
      default: setInnerValue({ No: null });
    }
  }

  return (
    <HStack>
      <Select
        w="95px"
        value={!!innerValue.Median ? "Median" : !!innerValue.Wavelet ? "Wavelet" : innerValue}
        bg="#689d6a"
        color="#32302f"
        border="unset"
        fontWeight="bold"
        onChange={onSelectChange}
        marginRight="9px"
      >
        <option value="No">无</option>
        <option value="Median">中值</option>
        <option value="Wavelet">小波</option>
      </Select>
      {!!innerValue.Median &&
        <Box w="200px" marginRight="9px">
          <IInput
            leftTag="窗口宽度"
            value={innerValue.Median}
            onBlur={v => {
              const vv = parseInt(v);
              if (isNaN(vv) || vv < 0) {
                setErrMsg(`不合法的窗口宽度：${v}`);
                return;
              }
              setInnerValue({ Median: vv });
            }}
            mutable
          />
        </Box>}
      {!!innerValue && !!innerValue.Wavelet &&
        <Box w="300px" marginRight="9px">
          <IInput
            leftTag="滤波阈值"
            value={innerValue.Wavelet.toPrecision(2)}
            onBlur={v => {
              const vv = parseFloat(v);
              if (isNaN(vv) || vv < 0 || vv > 1) {
                setErrMsg(`不合法的滤波阈值：${v}`);
                return;
              }
              setInnerValue({ Wavelet: vv });
            }}
            mutable
            rightTag="(0, 1)"
          />
        </Box>
      }
      <IButton
        text="滤波"
        hover="由于对全部数据点滤波耗时较长，此处仅对当前数据点进行滤波，完整滤波在求解时进行"
        onClick={() => onSubmit(innerValue)}
      />
    </HStack>
  )
}

export default SelectFilter