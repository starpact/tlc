import { useState, useEffect } from "react";
import {
  Table,
  Thead,
  Tbody,
  Tr,
  Th,
  Td,
  Box,
} from "@chakra-ui/react";
import * as tauri from 'tauri/api/tauri';

function ITable(setErrMsg) {
  const [daq, setDaq] = useState("");

  useEffect(() => {
    daq === "" && tauri.promisified({ cmd: "getDaq" })
      .then(ok => setDaq(ok))
      .catch(err => setErrMsg(err));
  }, []);

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
    </Box>
  )
}

export default ITable