import { useEffect, createRef } from 'react';

const Canvas = ({ draw }) => {
  const canvas = createRef();

  useEffect(() => {
    const ctx = canvas.current.getContext('2d');
    draw(ctx);
  }, [canvas, draw]);

  return <canvas ref={canvas} width="640" height="512" />;
};

export default Canvas;