// TypeScript, including the JavaScript syntax it inherits.
import { readFile } from "node:fs/promises";

export interface User {
  readonly id: number;
  name?: string;
}

type Role = "admin" | "user";

enum Level { Low = 1, High }

export class Store<T extends User> implements Iterable<T> {
  private items: T[] = [];
  constructor(private readonly role: Role) {}

  async load(path: string): Promise<void> {
    const text = await readFile(path, "utf8");
    this.items = JSON.parse(text) as T[];
  }

  *[Symbol.iterator]() {
    yield* this.items;
  }
}

const greet = (u: User): string => `Hello ${u.name ?? "you"}!`;
let pattern = /^[a-z]+$/gi;
console.log(greet({ id: 1 }), Level.High, typeof pattern, null, undefined, true);
