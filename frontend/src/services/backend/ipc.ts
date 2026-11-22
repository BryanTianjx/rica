// The IPC implementation of the positions branch (F1-2 vertical slice):
// every method maps to one ica-desktop command through the Tauri invoke
// boundary, served in-process by the host's own DesktopState — no HTTP hop.
//
// Error parity: the host wrapper returns `IpcError`, whose serialized form is
// the SAME {"error": {code, message, details}} envelope the HTTP transport
// emits (pinned by ica-desktop's commands_smoke test asserting field-level
// equality with the HTTP envelope). A rejected invoke therefore reconstructs
// an identical ApiError and pages keep their single error path.
//
// Arg naming: Tauri 2 converts command parameters to camelCase — every
// wrapped command here uses single-word parameters (`args`, `id`, `payload`),
// so the invoke keys match verbatim.
import { invoke } from "@tauri-apps/api/core";
import { ApiError } from "../api";
import type { Page, Position } from "../../types";
import type { PositionsBackend } from "./client";

/** What a rejected command hands over: the serialized IpcError envelope. */
interface IpcRejection {
  error?: { code?: string; message?: string; details?: Record<string, unknown> };
}

function toApiError(rejection: unknown): ApiError {
  const envelope = (rejection ?? {}) as IpcRejection;
  return new ApiError(
    0,
    {
      error: {
        code: envelope.error?.code ?? "unknown_error",
        message: envelope.error?.message ?? "The desktop command failed",
        details: envelope.error?.details ?? {},
      },
    },
    "The desktop command failed",
  );
}

/** List args as the host command expects (i64 page window). */
export interface PositionListInvokeArgs {
  include_archived?: boolean;
  search?: string;
  offset?: number;
  limit?: number;
}

async function invokeOrThrow<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (rejection) {
    throw toApiError(rejection);
  }
}

export const ipcPositions: PositionsBackend = {
  async list(params?: {
    search?: string;
    status?: string;
    position_id?: string;
    include_archived?: boolean;
    offset?: number;
    limit?: number;
  }) {
    const args: PositionListInvokeArgs = {};
    if (params?.include_archived !== undefined)
      args.include_archived = params.include_archived;
    if (params?.search) args.search = params.search;
    if (params?.offset !== undefined) args.offset = params.offset;
    if (params?.limit !== undefined) args.limit = params.limit;
    // The command returns the Page envelope; the transport's `total` parity
    // (IMP-1/IMP-3) is inherited from the shared ica-contracts type.
    return invokeOrThrow<Page<Position>>("positions_list", { args });
  },

  async get(id: string) {
    return invokeOrThrow<Position>("positions_get", { id });
  },

  async create(payload: Partial<Position> & { name: string }) {
    return invokeOrThrow<Position>("positions_create", { payload });
  },

  async update(id: string, payload: Partial<Position>) {
    return invokeOrThrow<Position>("positions_patch", { id, payload });
  },

  async remove(id: string): Promise<void> {
    await invokeOrThrow<void>("positions_delete", { id });
  },
};
