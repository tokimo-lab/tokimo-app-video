// TODO(phase3): NEED_SDK_TYPED_API_CLIENT
// Until the standalone app gets its own typed client to the OS /api/* endpoints
// and the local /api/apps/tokimo-video/* binary, every `api.*` call throws.

const handler: ProxyHandler<object> = {
  get(_t, prop): unknown {
    if (prop === Symbol.toPrimitive) return () => "[api-stub]";
    if (prop === "then") return undefined; // prevent accidental Promise resolution
    return new Proxy(() => {
      throw new Error(`NEED_SDK_TYPED_API_CLIENT: api.${String(prop)} called`);
    }, handler);
  },
};

export const api: any = new Proxy({}, handler);

export function useApiQuery(): any {
  throw new Error("NEED_SDK_TYPED_API_CLIENT: useApiQuery called");
}

export function useApiMutation(): any {
  throw new Error("NEED_SDK_TYPED_API_CLIENT: useApiMutation called");
}

// Re-export types that might be imported
export type VideoOutput = any;
export type FileProbeStream = any;
export type ApiTypes = any;
