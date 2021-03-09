import * as echarts from "echarts";
import { useEffect, useRef } from "react";

function GreenHistoryLine({ history, pos }) {
  const myCharts = useRef(null);

  useEffect(() => {
    if (!myCharts.current) myCharts.current = echarts.init(document.getElementById("history"));
    myCharts.current.setOption({
      title: {
        text: `绿色通道历史(x: ${pos[0]} y: ${pos[1]})`,
        textStyle: {
          color: "#fbf1c7",
        },
        x: "center"
      },
      tooltip: {
        trigger: "axis",
      },
      xAxis: {
        data: history.map((_, i) => i + 1),
      },
      yAxis: {
        type: "value"
      },
      color: "#98971a",
      textStyle: {
        color: "#fbf1c7",
      },
      dataZoom: [{
        show: true,
        type: "slider",
      }],
      grid: {
        show: true,
        top: "15%",
        left: "13%",
        right: "2%",
        bottom: "35%",
      },
      series: [
        {
          type: "line",
          data: history,
        }
      ]
    });
  }, [history]);

  return (
    <div
      id="history"
      style={{ width: "800px", height: "200px" }}
    >
    </div>
  )
}

export default GreenHistoryLine