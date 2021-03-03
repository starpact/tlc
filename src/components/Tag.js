import { Box, Text } from "@chakra-ui/react";

function ITag({ text, w }) {
  return (
    <Box
      textAlign="center"
      rounded="md"
      w={w}
      bgColor="#98971a"
    >
      <Text color="#32302f" fontWeight="bold">
        {text}
      </Text>
    </Box>
  )
}

export default ITag