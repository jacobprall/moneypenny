export const ErrorCodes = {
  VALIDATION_FAILED: "VALIDATION_FAILED",
  SESSION_NOT_FOUND: "SESSION_NOT_FOUND",
  SESSION_WRONG_STATE: "SESSION_WRONG_STATE",
  CONFIG_VERSION_MISMATCH: "CONFIG_VERSION_MISMATCH",
  BLUEPRINT_NOT_FOUND: "BLUEPRINT_NOT_FOUND",
  BLUEPRINT_INVALID: "BLUEPRINT_INVALID",
  TOOL_NOT_FOUND: "TOOL_NOT_FOUND",
  PERMISSION_DENIED: "PERMISSION_DENIED",
  BUDGET_EXCEEDED: "BUDGET_EXCEEDED",
  IDEA_NOT_FOUND: "IDEA_NOT_FOUND",
  RUN_NOT_FOUND: "RUN_NOT_FOUND",
  MESSAGE_NOT_FOUND: "MESSAGE_NOT_FOUND",
  TAB_NOT_FOUND: "TAB_NOT_FOUND",
  FILE_NOT_FOUND: "FILE_NOT_FOUND",
  BINARY_FILE_REJECTED: "BINARY_FILE_REJECTED",
  INTERNAL: "INTERNAL",
} as const;

export type ErrorCode = (typeof ErrorCodes)[keyof typeof ErrorCodes];

const STATUS: Partial<Record<ErrorCode, number>> = {
  VALIDATION_FAILED: 400,
  SESSION_NOT_FOUND: 404,
  IDEA_NOT_FOUND: 404,
  RUN_NOT_FOUND: 404,
  MESSAGE_NOT_FOUND: 404,
  TAB_NOT_FOUND: 404,
  FILE_NOT_FOUND: 404,
  BLUEPRINT_NOT_FOUND: 404,
  TOOL_NOT_FOUND: 404,
  CONFIG_VERSION_MISMATCH: 409,
  SESSION_WRONG_STATE: 409,
  BINARY_FILE_REJECTED: 400,
  PERMISSION_DENIED: 422,
  BUDGET_EXCEEDED: 422,
};

export class MoneypennyError extends Error {
  readonly code: ErrorCode;
  readonly status: number;
  readonly details?: unknown;

  constructor(
    code: ErrorCode,
    message: string,
    opts?: { details?: unknown; status?: number },
  ) {
    super(message);
    this.code = code;
    this.details = opts?.details;
    this.status = opts?.status ?? STATUS[code] ?? 500;
  }
}

export function errorToJson(err: MoneypennyError): {
  error: { code: string; message: string; details?: unknown };
} {
  return {
    error: {
      code: err.code,
      message: err.message,
      ...(err.details !== undefined ? { details: err.details } : {}),
    },
  };
}
