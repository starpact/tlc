import { Box, HStack, Select } from "@chakra-ui/react";
import { useState } from "react";
import IButton from "./Button";
import IInput from "./Input";

function SelectFilter({ value, onSubmit, awsl }) {
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
        w="110px"
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
              if (!vv || vv < 0) {
                awsl(`不合法的窗口宽度：${v}`);
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
              if (!vv || vv < 0 || vv > 1) {
                awsl(`不合法的滤波阈值：${v}`);
                return;
              }
              setInnerValue({ Wavelet: vv });
            }}
            mutable
            rightTag="(0, 1)"
          />
        </Box>
      }
      <IButton text="提交" onClick={() => onSubmit(innerValue)} />
    </HStack>
  )
}

export default SelectFilter