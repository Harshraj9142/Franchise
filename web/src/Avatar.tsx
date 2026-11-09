import type { ReactElement } from "react";

/** A deterministic geometric mark per agent, so every NFT is recognisable without uploaded art. */
function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return h >>> 0;
}

export function Avatar({ seed, size = 56 }: { seed: string; size?: number }) {
  const h = hash(seed);
  const hue = h % 360;
  const cells: ReactElement[] = [];
  for (let y = 0; y < 5; y++)
    for (let x = 0; x < 3; x++) {
      if ((h >> (y * 3 + x)) & 1) {
        cells.push(<rect key={`${x}-${y}`} x={x * 6 + 3} y={y * 6 + 3} width="6" height="6" />);
        if (x < 2) cells.push(<rect key={`m${x}-${y}`} x={(4 - x) * 6 + 3} y={y * 6 + 3} width="6" height="6" />);
      }
    }
  return (
    <svg className="avatar" width={size} height={size} viewBox="0 0 36 36" aria-hidden>
      <rect width="36" height="36" rx="8" fill={`hsl(${hue} 55% 92%)`} />
      <g fill={`hsl(${hue} 60% 32%)`}>{cells}</g>
    </svg>
  );
}
