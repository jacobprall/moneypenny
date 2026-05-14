import type { Skill, SubagentDef } from "@moneypenny/db/types";

/**
 * Hardcoded default skills have been replaced by file-based bundled skills
 * in packages/skills/bundled/. Skills are now loaded from disk via scanSkillDirs()
 * at session startup and stored in the DB.
 *
 * These empty arrays are kept for backward compatibility with DEFAULT_AGENT_DEF.
 */

export const DEFAULT_SKILLS: Skill[] = [];

export const DEFAULT_SUBAGENT_DEFS: SubagentDef[] = [];
