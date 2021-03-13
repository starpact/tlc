import * as echarts from "echarts";
import { useEffect, useRef } from "react";

const SCALING = 5;

function InterpDistribution({ interp, setPos, w, h }) {
  const myCharts = useRef(null);

  useEffect(() => {
    if (!myCharts.current) myCharts.current = echarts.init(document.getElementById("interp"));

    const data = [];
    const xData = [];
    const yData = [];
    let [minT, maxT] = [interp.data[0], interp.data[0]];
    for (let i = 0; i < interp.dim[0]; i++) {
      for (let j = 0; j < interp.dim[1]; j++) {
        const t = interp.data[i * interp.dim[1] + j];
        minT = Math.min(t, minT);
        maxT = Math.max(t, maxT);
        data.push([j, i, t]);
      }
    }
    for (let i = interp.dim[0] - 1; i >= 0; i--) {
      yData.push(i * SCALING);
    }
    for (let j = 0; j < interp.dim[1]; j++) {
      xData.push(j * SCALING);
    }

    myCharts.current.setOption({
      title: {
        text: "参考温度插值",
        textStyle: {
          color: "#fbf1c7",
        },
        x: 375
      },
      tooltip: {
        trigger: "item",
        backgroundColor: "#fbf1c7",
        textStyle: {
          fontStyle: "bold"
        },
        formatter: function (p) {
          return `参考温度: ${p.data[2].toFixed(2)}`;
        },
        axisPointer: {
          type: "cross",
        },
      },
      grid: {
        top: 50,
        left: 110,
        right: 20,
        bottom: 10,
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
        min: minT,
        max: maxT,
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
        name: '参考温度',
        type: 'heatmap',
        data: data,
        progressive: 800,
        animation: false,
      }]
    });

    myCharts.current.on("click", params => {
      const [x, y] = params.data;
      setPos([x * SCALING, (interp.dim[0] - y - 1) * SCALING]);
    });
  }, [interp]);

  return (
    <div
      id="interp"
      style={{ width: w + 130, height: h + 60 }}
    >
    </div>
  )
}

export default InterpDistribution