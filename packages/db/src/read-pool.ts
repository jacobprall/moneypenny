import { Database } from "bun:sqlite";
import { sqlError } from "./errors.js";

/**
 * Small pool of read-only connections to the same SQLite file (WAL-safe readers).
 */
export class DbReadPool {
  private readonly connections: Database[] = [];
  private next = 0;

  constructor(
    dbPath: string,
    poolSize: number,
  ) {
    const n = Math.max(1, Math.min(4, poolSize));
    for (let i = 0; i < n; i++) {
      let database: Database;
      try {
        database = new Database(dbPath, { readonly: true, create: false });
      } catch (e) {
        for (const c of this.connections) {
          try {
            c.close();
          } catch {
            /* ignore */
          }
        }
        this.connections.length = 0;
        throw sqlError("open read-pool database", e);
      }
      try {
        database.exec(`PRAGMA foreign_keys=ON;`);
      } catch (e) {
        try {
          database.close();
        } catch {
          /* ignore */
        }
        for (const c of this.connections) {
          try {
            c.close();
          } catch {
            /* ignore */
          }
        }
        this.connections.length = 0;
        throw sqlError("configure read-pool PRAGMAs", e);
      }
      this.connections.push(database);
    }
  }

  read<T>(fn: (db: Database) => T): T {
    const conn = this.connections[this.next % this.connections.length]!;
    this.next++;
    return fn(conn);
  }

  close(): void {
    for (const c of this.connections) {
      try {
        c.close();
      } catch {
        /* ignore */
      }
    }
    this.connections.length = 0;
  }
}
