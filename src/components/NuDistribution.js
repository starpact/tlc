import { useEffect, useRef } from "react";

function Nu2dDistribution({ result, w, h }) {
  const canvas = useRef(null);

  useEffect(() => draw(), [result]);

  function draw() {
    const ctx = canvas.current.getContext("2d");
    if (!result) {
      ctx.fillStyle = "#3c3836";
      ctx.fillRect(0, 0, w, h);
      ctx.fillStyle = "#fbf1c7";
      ctx.font = "30px serif";
      ctx.fillText("待求解", w / 2 - 50, h / 2);
      return;
    }

    const img = new Image();
    img.src = `data: image/png;base64,${result}`;
    img.onload = () => ctx.drawImage(img, 0, 0, w, h);
  }

  return (
    <canvas
      width={w}
      height={h}
      ref={canvas}
    >
    </canvas>
  )
}

export default Nu2dDistribution