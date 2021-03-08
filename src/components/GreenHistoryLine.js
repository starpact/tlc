import "echarts/lib/chart/line";
import "echarts/lib/component/tooltip";
import "echarts/lib/component/title";
import "echarts/lib/component/legend";
import "echarts/lib/component/markPoint";
import ReactEcharts from "echarts-for-react";

function GreenHistoryLine({ history, pos }) {
  function getOption() {
    const option = {
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
        left: "4%",
        right: "0%",
        bottom: "35%",
      },
      series: [
        {
          type: "line",
          data: history,
        }
      ]
    };

    return option;
  }

  return (
    <div>
      <ReactEcharts
        option={getOption()}
        style={{ width: "800px", height: "200px" }}
      />
    </div>
  )
}

export default GreenHistoryLine