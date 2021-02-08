import { IconButton } from "@chakra-ui/react";

function IIConButton({ icon, onClick }) {
  return (
    <IconButton
      size="sm"
      icon={icon}
      backgroundColor="#d79921"
      color="#282828"
      onClick={onClick}
    />
  )
}

export default IIConButton
