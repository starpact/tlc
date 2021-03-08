import "echarts/lib/chart/line";
import "echarts/lib/component/tooltip";
import "echarts/lib/component/title";
import "echarts/lib/component/legend";
import "echarts/lib/component/markPoint";
import ReactEcharts from "echarts-for-react";

const SCALING = 5;

function InterpDistribution({ interp, setPos }) {
  function getOption() {
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

    const option = {
      title: {
        text: "参考温度插值",
        textStyle: {
          color: "#fbf1c7",
        },
        x: "center"
      },
      tooltip: {
        trigger: "item",
        formatter: function (p) {
          return "参考温度: " + p.data[2].toFixed(2);
        },
        axisPointer: {
          type: "cross"
        },
      },
      grid: {
        top: "20%",
        left: "13%",
        right: "0%",
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
        progressive: 600,
        animation: false,
      }]
    };

    return option;
  }

  const onEvents = {
    "click": params => {
      const [x, y] = params.data
      setPos([x * SCALING, (interp.dim[0] - y - 1) * SCALING]);
    },
  };

  return (
    <div>
      <ReactEcharts
        option={getOption()}
        onEvents={onEvents}
        style={{ width: "800px", height: "270px" }}
      />
    </div>
  )
}

export default InterpDistribution