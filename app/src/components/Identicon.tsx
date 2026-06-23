"use client";

import { generateIdenticon } from "@/lib/identicon";

interface IdenticonProps {
  seed: string;
  size?: number;
  cellSize?: number;
}

/** Small pixel-grid avatar badge derived deterministically from `seed` (e.g. a Stellar address). */
export function Identicon({ seed, size = 5, cellSize = 3 }: IdenticonProps) {
  const grid = generateIdenticon(seed, size);
  const px = grid.size * cellSize;

  return (
    <div
      className="pixel-border-thin"
      style={{
        width: `${px}px`,
        height: `${px}px`,
        background: grid.bg,
        display: "grid",
        gridTemplateColumns: `repeat(${grid.size}, ${cellSize}px)`,
        gridTemplateRows: `repeat(${grid.size}, ${cellSize}px)`,
        imageRendering: "pixelated",
      }}
    >
      {grid.cells.flatMap((row, y) =>
        row.map((on, x) => (
          <div key={`${x}-${y}`} style={{ background: on ? grid.fg : "transparent" }} />
        ))
      )}
    </div>
  );
}
