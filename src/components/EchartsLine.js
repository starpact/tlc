import React from "react";
import "echarts/lib/chart/line";
import "echarts/lib/component/tooltip";
import "echarts/lib/component/title";
import "echarts/lib/component/legend";
import "echarts/lib/component/markPoint";
import ReactEcharts from "echarts-for-react";

function EchartsLine({
  daq,
  scrollToColumn,
  setScrollToRow
}) {
  function getOption() {
    if (!daq) return;
    const yData = [];
    if (scrollToColumn >= 0) {
      for (let i = scrollToColumn; i < daq.data.length; i += daq.dim[1]) {
        yData.push(daq.data[i]);
      }
    }
    const xData = yData.map((_, i) => i);
    const title = scrollToColumn >= 0 ? `第${scrollToColumn}列` : "请选择需要预览的列";

    let option = {
      title: {
        text: title,
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
      series: [
        {
          type: "line",
          data: yData,
        }
      ]
    };

    return option;
  }

  return (
    <div>
      <ReactEcharts
        option={getOption()}
        style={{ width: "900px", height: "225px" }}
      />
    </div>
  )
}

export default EchartsLine