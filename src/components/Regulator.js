import {
  Slider,
  SliderTrack,
  SliderThumb,
  SliderFilledTrack,
  Text,
  Center,
  Tag,
  Stack,
  HStack,
} from "@chakra-ui/react"
import { useState } from "react";
import IButton from "./Button";

function ISlider({ value, onChange }) {
  return (
    <Stack>
      <Slider
        defaultValue={100}
        min={50}
        max={150}
        onChange={v => onChange(v / 100)}
        orientation="vertical"
        value={value * 100}
        h="100px"
      >
        <SliderTrack bgColor="#665c54">
          <SliderFilledTrack bgColor="#98971a" />
        </SliderTrack>
        <SliderThumb bgColor="#928374" />
      </Slider>
      <Tag size="lg" bgColor="#98971a">
        <Text color="#32302f" fontWeight="bold">
          {value.toFixed(2)}
        </Text>
      </Tag>
    </Stack >
  )
}

function Regulator({ regulator, onSubmit }) {
  const [innerRegulator, setInnerRegulator] = useState(regulator);

  return (
    <HStack>
      <Center>
        {!!innerRegulator && innerRegulator.map((v, i) =>
          <ISlider
            key={i}
            value={v}
            onChange={v => {
              const arr = innerRegulator.concat();
              arr[i] = v;
              setInnerRegulator(arr);
            }}
          />
        )}
      </Center>
      <Stack>
        <IButton text="重置" onClick={() => setInnerRegulator(innerRegulator.map(() => 1.0))} />
        <IButton text="提交" onClick={() => onSubmit(innerRegulator)} />
      </Stack>
    </HStack>
  )
}

export default Regulator
