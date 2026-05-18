import { readFileSync, writeFileSync } from "node:fs";
import yaml from "js-yaml";
import type { Idea, IdeaLink, IdeaSource } from "./types.js";

function readFm(raw: string): { fm: Record<string, unknown>; body: string } | { error: string } {
  const m = raw.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n([\s\S]*)$/);
  if (!m) return { error: "missing frontmatter" };
  try {
    const fm = yaml.load(m[1]) as Record<string, unknown>;
    if (!fm || typeof fm !== "object") return { error: "frontmatter must be object" };
    return { fm, body: m[2].replace(/\s+$/, "") };
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
}

function linksFrom(v: unknown): IdeaLink[] | undefined {
  if (!Array.isArray(v)) return undefined;
  const out: IdeaLink[] = [];
  for (const x of v) {
    if (!x || typeof x !== "object") continue;
    const o = x as { type?: unknown; id?: unknown; note?: unknown };
    if (typeof o.type === "string" && typeof o.id === "string") {
      out.push({
        type: o.type,
        id: o.id,
        note: typeof o.note === "string" ? o.note : undefined,
      });
    }
  }
  return out.length ? out : undefined;
}

export function parseIdeaFile(
  path: string,
  source: IdeaSource,
  filename: string,
): Idea | { error: string } {
  let raw: string;
  try {
    raw = readFileSync(path, "utf-8");
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
  return parseIdeaContent(raw, { filename, path, source });
}

export function parseIdeaContent(
  raw: string,
  meta: { filename: string; path: string; source: IdeaSource },
): Idea | { error: string } {
  const parsed = readFm(raw);
  if ("error" in parsed) return parsed;
  const { fm, body } = parsed;
  const extra: Record<string, unknown> = {};
  let title: string | undefined;
  let status: string | undefined;
  let priority: string | undefined;
  let tags: string[] | undefined;
  let spec_session_id: string | null | undefined;
  let impl_session_ids: string[] | undefined;
  let created_at: string | undefined;
  let updated_at: string | undefined;
  let links: IdeaLink[] | undefined;

  for (const [k, v] of Object.entries(fm)) {
    switch (k) {
      case "title":
        title = typeof v === "string" ? v : undefined;
        break;
      case "status":
        status = typeof v === "string" ? v : undefined;
        break;
      case "priority":
        priority = typeof v === "string" ? v : undefined;
        break;
      case "tags":
        tags = Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : undefined;
        break;
      case "spec_session_id":
        spec_session_id = v === null ? null : typeof v === "string" ? v : undefined;
        break;
      case "impl_session_ids":
        impl_session_ids = Array.isArray(v)
          ? v.filter((x): x is string => typeof x === "string")
          : undefined;
        break;
      case "created_at":
        created_at = typeof v === "string" ? v : undefined;
        break;
      case "updated_at":
        updated_at = typeof v === "string" ? v : undefined;
        break;
      case "links":
        links = linksFrom(v);
        break;
      default:
        extra[k] = v;
    }
  }

  const stem = meta.filename.replace(/\.md$/i, "");
  const idea: Idea = {
    filename: meta.filename.endsWith(".md") ? meta.filename : `${meta.filename}.md`,
    path: meta.path,
    source: meta.source,
    title: title?.trim() ? title : stem,
    status: status ?? "raw",
    body,
    extra,
  };
  if (priority !== undefined) idea.priority = priority;
  if (tags !== undefined) idea.tags = tags;
  if (spec_session_id !== undefined) idea.spec_session_id = spec_session_id;
  if (impl_session_ids !== undefined) idea.impl_session_ids = impl_session_ids;
  if (created_at !== undefined) idea.created_at = created_at;
  if (updated_at !== undefined) idea.updated_at = updated_at;
  if (links !== undefined) idea.links = links;
  return idea;
}

export function serializeIdea(idea: Idea): string {
  const fm: Record<string, unknown> = {
    title: idea.title,
    status: idea.status,
  };
  if (idea.priority !== undefined) fm.priority = idea.priority;
  if (idea.tags !== undefined) fm.tags = idea.tags;
  if (idea.spec_session_id !== undefined) fm.spec_session_id = idea.spec_session_id;
  if (idea.impl_session_ids !== undefined) fm.impl_session_ids = idea.impl_session_ids;
  if (idea.created_at !== undefined) fm.created_at = idea.created_at;
  if (idea.updated_at !== undefined) fm.updated_at = idea.updated_at;
  if (idea.links !== undefined) fm.links = idea.links;
  for (const k of Object.keys(idea.extra).sort()) {
    fm[k] = idea.extra[k];
  }
  const head = yaml.dump(fm, { lineWidth: 120, noRefs: true }).trimEnd();
  return `---\n${head}\n---\n\n${idea.body.trimEnd()}\n`;
}

export function writeIdeaFile(path: string, idea: Idea): void {
  writeFileSync(path, serializeIdea(idea), "utf-8");
}
