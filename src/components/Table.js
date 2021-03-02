import { useState, useEffect } from "react";
import {
  Text,
  Table,
  Thead,
  Tbody,
  Tr,
  Th,
  Td,
  Box,
} from "@chakra-ui/react";
import * as tauri from 'tauri/api/tauri';

function ITable(config, setErrMsg) {
  const [daq, setDaq] = useState("");

  useEffect(() => {
    tauri.promisified({ cmd: "getDaq" })
      .then(ok => setDaq(ok))
      .catch(err => setErrMsg(err));
  }, [config]);

  return (
    <Box>
      <Table w="600px" h="400px" variant="simple" colorScheme="teal">
        <Thead>
          <Tr>
            <Th>To convert</Th>
            <Th>into</Th>
          </Tr>
        </Thead>
        {/* <Tbody>
          {
            !!daq && daq.data.map(v =>
              <Tr>
                <Td>{v}</Td>
              </Tr>
            )
          }
        </Tbody> */}
      </Table>
      <Text color="red">{!!daq && daq.data.length}</Text>
    </Box>
  )
}

export default ITable