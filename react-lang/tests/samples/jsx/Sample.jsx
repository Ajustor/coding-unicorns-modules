import React, { useEffect } from "react";

// A list.
export default function List({ items = [], onPick }) {
  useEffect(() => {
    document.title = `${items.length} items`;
  }, [items]);
  return (
    <ul className="list">
      {items.map((item) => (
        <li key={item.id} onClick={() => onPick(item)}>
          {item.label} <strong>{item.count > 1 ? "many" : "one"}</strong>
        </li>
      ))}
    </ul>
  );
}
