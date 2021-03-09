import * as echarts from "echarts";
import { useEffect, useRef } from "react";

const SCALING = 1;

function Nu2dDistribution({ result }) {
  const myCharts = useRef(null);

  useEffect(() => {
    if (!myCharts.current) myCharts.current = echarts.init(document.getElementById("nu2d"));

    const [nu2d, nuAve] = result;
    const data = [];
    const xData = [];
    const yData = [];
    for (let i = 0; i < nu2d.dim[0]; i++) {
      for (let j = 0; j < nu2d.dim[1]; j++) {
        data.push([j, i, nu2d.data[i * nu2d.dim[1] + j]]);
      }
    }
    for (let i = nu2d.dim[0] - 1; i >= 0; i--) {
      yData.push(i * SCALING);
    }
    for (let j = 0; j < nu2d.dim[1]; j++) {
      xData.push(j * SCALING);
    }

    myCharts.current.setOption({
      title: {
        text: "努塞尔数分布",
        textStyle: {
          color: "#fbf1c7",
        },
        x: "center"
      },
      tooltip: {
        trigger: "item",
        formatter: function (p) {
          return "Nu: " + p.data[2].toFixed(2);
        },
        axisPointer: {
          type: "cross"
        },
      },
      grid: {
        top: "20%",
        left: "13%",
        right: "2%",
        bottom: "2%",
      },
      xAxis: {
        type: 'category',
        position: "top",
        data: xData
      },
      yAxis: {
        type: 'category',
        data: yData
      },
      visualMap: {
        textStyle: {
          color: "#fbf1c7",
        },
        type: "continuous",
        precision: 2,
        top: "bottom",
        align: "right",
        min: nuAve * 0.6,
        max: nuAve * 2,
        calculable: true,
        realtime: false,
        inRange: {
          color: [
            "rgb(0,0,128)", "rgb(0,0,255)", "rgb(0,128,255)", "rgb(0,252,255)",
            "rgb(130,255,126)", "rgb(255,252,0)", "rgb(255,128,0)", "rgb(252,0,0)",
            "rgb(128,0,0)"
          ]
        }
      },
      series: [{
        name: '努塞尔数',
        type: 'heatmap',
        data: data,
        progressive: 800,
        animation: false,
      }]
    });
  }, [result]);

  return (
    <div
      id="nu2d"
      style={{ width: "800px", height: "400px" }}
    >
    </div>
  )
}

export default Nu2dDistribution