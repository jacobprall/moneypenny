/**
 * A size-bounded Map that evicts the least-recently-inserted entry when full.
 * Used to cache compiled RegExp objects without unbounded memory growth.
 */
export class BoundedMap<K, V> {
  private readonly map = new Map<K, V>();
  private readonly capacity: number;

  constructor(capacity: number) {
    if (capacity < 1) throw new Error("BoundedMap capacity must be >= 1");
    this.capacity = capacity;
  }

  get(key: K): V | undefined {
    return this.map.get(key);
  }

  has(key: K): boolean {
    return this.map.has(key);
  }

  set(key: K, value: V): void {
    if (this.map.has(key)) {
      this.map.set(key, value);
      return;
    }
    if (this.map.size >= this.capacity) {
      const firstKey = this.map.keys().next().value;
      if (firstKey !== undefined) this.map.delete(firstKey);
    }
    this.map.set(key, value);
  }

  get size(): number {
    return this.map.size;
  }

  clear(): void {
    this.map.clear();
  }
}
