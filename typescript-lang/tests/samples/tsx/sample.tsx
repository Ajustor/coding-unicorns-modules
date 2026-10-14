import { useState } from "react";

type Props = { label: string; start?: number };

export function Counter({ label, start = 0 }: Props) {
  const [count, setCount] = useState<number>(start);
  return (
    <div className="counter">
      <span>{label}: {count}</span>
      <button onClick={() => setCount(count + 1)} disabled={count > 9}>
        +1
      </button>
    </div>
  );
}
