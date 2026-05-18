export function assertLocalBind(): void {
  const b = process.env.MP_BIND?.trim();
  if (!b || b === "127.0.0.1" || b === "localhost" || b === "::1") return;
  throw new Error(
    `Refusing to start: MP_BIND must be 127.0.0.1, localhost, ::1, or unset (got ${b})`,
  );
}
