# Verification System

### VerifySpec

```typescript
export type VerifySpec =
  | { type: "command"; command: string; cwd?: string }
  | { type: "pytest"; testPath: string; args?: string[] }
  | { type: "grep-absent"; pattern: string; glob?: string }
  | { type: "composite"; checks: VerifySpec[] }
  | { type: "patch-then-test"; testPatch: string; testCommand: string };  // NEW
```

### SWE-bench test patch application

**Problem identified in gap analysis:** The `makeVerifySpec` function
didn't actually apply the test patch. In real SWE-bench evaluation, the
test patch contains **new tests** that verify the fix. These must be
applied before running the test suite — otherwise you're running the
old tests that already pass.

The new `patch-then-test` verify type handles this:

```typescript
async function verifyPatchThenTest(
  spec: { testPatch: string; testCommand: string },
  workdir: string,
  timeoutMs: number,
): Promise<VerifyResult> {
  // 1. Write the test patch to a temp file
  const patchFile = path.join(workdir, ".mp-eval-test.patch");
  await Bun.write(patchFile, spec.testPatch);

  // 2. Apply the test patch
  const applyResult = await execWithTimeout(
    "git", ["apply", "--check", patchFile],
    { cwd: workdir, timeoutMs: 10_000 },
  );

  if (applyResult.exitCode !== 0) {
    // Patch might conflict with agent's changes — try with 3-way merge
    const apply3way = await execWithTimeout(
      "git", ["apply", "--3way", patchFile],
      { cwd: workdir, timeoutMs: 10_000 },
    );
    if (apply3way.exitCode !== 0) {
      return {
        passed: false,
        output: `Test patch failed to apply: ${apply3way.stderr}`,
      };
    }
  } else {
    await execWithTimeout(
      "git", ["apply", patchFile],
      { cwd: workdir, timeoutMs: 10_000 },
    );
  }

  // 3. Run the test command
  const testResult = await execWithTimeout(
    "bash", ["-c", spec.testCommand],
    { cwd: workdir, timeoutMs },
  );

  // 4. Clean up patch file
  try { unlinkSync(patchFile); } catch {}

  return {
    passed: testResult.exitCode === 0,
    output: testResult.stdout + "\n" + testResult.stderr,
  };
}
```

### Updated `makeVerifySpec` for SWE-bench

```typescript
function makeVerifySpec(instance: SweBenchInstance): VerifySpec {
  const testFiles = extractTestFiles(instance.test_patch);
  const failToPass = instance.FAIL_TO_PASS
    ? JSON.parse(instance.FAIL_TO_PASS) as string[]
    : [];

  let testCommand: string;
  if (failToPass.length > 0) {
    testCommand = `python -m pytest ${failToPass.join(" ")} --tb=short -q`;
  } else if (testFiles.length > 0) {
    testCommand = `python -m pytest ${testFiles.join(" ")} --tb=short -q`;
  } else {
    testCommand = "python -m pytest --tb=short -q";
  }

  return {
    type: "patch-then-test",
    testPatch: instance.test_patch,
    testCommand,
  };
}
```

### Standard verification execution

```typescript
export async function runVerify(
  spec: VerifySpec,
  workdir: string,
  timeoutMs: number,
): Promise<VerifyResult> {
  switch (spec.type) {
    case "command":
      return execCommand(spec.command, spec.cwd ? path.join(workdir, spec.cwd) : workdir, timeoutMs);

    case "pytest": {
      const extra = spec.args?.join(" ") ?? "";
      return execCommand(`python -m pytest ${spec.testPath} ${extra} --tb=short -q`, workdir, timeoutMs);
    }

    case "grep-absent": {
      const glob = spec.glob ?? "**/*";
      const result = await execWithTimeout(
        "rg", ["--count", spec.pattern, "--glob", glob],
        { cwd: workdir, timeoutMs },
      );
      const count = result.stdout
        .split("\n")
        .filter(l => l.includes(":"))
        .reduce((sum, l) => sum + parseInt(l.split(":").pop() ?? "0", 10), 0);
      return { passed: count === 0, output: result.stdout };
    }

    case "composite": {
      const outputs: string[] = [];
      for (const check of spec.checks) {
        const sub = await runVerify(check, workdir, timeoutMs);
        outputs.push(sub.output);
        if (!sub.passed) return { passed: false, output: outputs.join("\n---\n") };
      }
      return { passed: true, output: outputs.join("\n---\n") };
    }

    case "patch-then-test":
      return verifyPatchThenTest(spec, workdir, timeoutMs);
  }
}
```

### Acceptance criteria

- [ ] `command` verify type runs shell commands, passes on exit code 0
- [ ] `pytest` verify type runs pytest on specified test files
- [ ] `grep-absent` verify type correctly detects (or not) patterns in files
- [ ] `composite` verify type runs checks in order, short-circuits on failure
- [ ] `patch-then-test` applies the test patch before running tests
- [ ] Test patch application falls back to 3-way merge on conflict
- [ ] All verification uses `execWithTimeout` with proper cleanup

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 4.1 | `VerifySpec` types, `runVerify` dispatcher, `execCommand` | 1 day |
| 4.2 | `patch-then-test` verify type with 3-way merge fallback | 1 day |
| 4.3 | Composite verification, grep-absent logic | 0.5 days |
