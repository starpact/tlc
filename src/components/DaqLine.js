import * as echarts from 'echarts';
import { useEffect, useRef } from "react";

function DaqLine({
  daq,
  scrollToColumn,
  setScrollToRow,
}) {
  const myCharts = useRef(null);

  useEffect(() => {
    if (!myCharts.current) myCharts.current = echarts.init(document.getElementById("daqLine"));

    const yData = [];
    if (scrollToColumn >= 0) {
      for (let i = scrollToColumn; i < daq.data.length; i += daq.dim[1]) {
        yData.push(daq.data[i]);
      }
    }
    const xData = yData.map((_, i) => i + 1);
    const option = {
      title: {
        text: scrollToColumn >= 0 ? `第${scrollToColumn + 1}列` : "请选择需要预览的列",
        textStyle: {
          color: "#fbf1c7",
        },
        x: "center"
      },
      tooltip: {
        trigger: "axis",
      },
      xAxis: {
        data: xData,
      },
      yAxis: {
        type: "value"
      },
      color: "#d79921",
      textStyle: {
        color: "#fbf1c7",
      },
      dataZoom: [{
        show: scrollToColumn >= 0,
        type: "slider",
      }],
      grid: {
        show: true,
        top: 30,
        left: "5%",
        right: "5%",
      },
      series: [
        {
          type: "line",
          data: yData,
        }
      ]
    };

    myCharts.current.setOption(option);
    myCharts.current.on("click", params => setScrollToRow(params.dataIndex));
  }, [scrollToColumn]);

  return (
    <div
      id="daqLine"
      style={{ width: "900px", height: "225px" }}
    >
    </div>
  )
}


export default DaqLine