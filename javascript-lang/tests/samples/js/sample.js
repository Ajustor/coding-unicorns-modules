// JavaScript
import { join } from "node:path";

export class Queue extends Array {
  #size = 0;
  static from(items) { return new Queue(...items); }
  get size() { return this.#size; }
}

async function main(args = []) {
  const { a, ...rest } = { a: 1, b: 2 };
  for (const x of [1, 2.5, 0xff, 1e3, 10n]) {
    if (x > 2 && x !== 255) continue;
  }
  try {
    await fetch(`https://example.org/${join("a", "b")}`);
  } catch (err) {
    throw new Error(err?.message ?? "failed");
  } finally {
    console.log(rest, /\d+/.test("42"), null, undefined, true, this);
  }
}

module.exports = { main };
