import React from "react";
import "echarts/lib/chart/line";
import "echarts/lib/component/tooltip";
import "echarts/lib/component/title";
import "echarts/lib/component/legend";
import "echarts/lib/component/markPoint";
import ReactEcharts from "echarts-for-react";

function DaqLine({
  daq,
  scrollToColumn,
  setScrollToRow,
}) {
  function getOption() {
    const yData = [];
    if (scrollToColumn >= 0) {
      for (let i = scrollToColumn; i < daq.data.length; i += daq.dim[1]) {
        yData.push(daq.data[i]);
      }
    }
    const xData = yData.map((_, i) => i + 1);
    const title = scrollToColumn >= 0 ? `第${scrollToColumn + 1}列` : "请选择需要预览的列";
    const show = scrollToColumn >= 0;

    const option = {
      title: {
        text: title,
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
        show,
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

    return option;
  }

  const onEvents = {
    "click": (params) => setScrollToRow(params.dataIndex),
  };

  return (
    <div>
      <ReactEcharts
        option={getOption()}
        onEvents={onEvents}
        style={{ width: "900px", height: "225px" }}
      />
    </div>
  )
}

export default DaqLine