/**
 * Compile a user-supplied regex pattern with ReDoS safeguards.
 * Rejects patterns that are too long or contain known catastrophic backtracking constructs.
 */

const MAX_PATTERN_LENGTH = 512;

const REDOS_INDICATORS = [
  /\(.*\+\).*\+/,   // nested quantifiers like (a+)+
  /\(.*\*\).*\*/,   // nested quantifiers like (a*)*
  /\(.*\+\).*\*/,   // (a+)*
  /\(.*\*\).*\+/,   // (a*)+
  /\(.*\{.*\}\).*[+*{]/, // (a{n})+
];

export function compileUserRegex(pattern: string): RegExp | null {
  if (pattern.length > MAX_PATTERN_LENGTH) return null;

  for (const indicator of REDOS_INDICATORS) {
    if (indicator.test(pattern)) return null;
  }

  try {
    return new RegExp(pattern);
  } catch {
    return null;
  }
}
