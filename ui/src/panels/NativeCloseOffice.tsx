import { useEffect } from "react";
import type { AgentId, KillResult } from "../protocol";
import type { KitiStore } from "../state/store";

type Props = { store: KitiStore; killAgent: (id: AgentId) => Promise<KillResult> };

/**
 * En la app nativa confirma Rust con su diálogo: este componente solo pasa cada petición de
 * cierre a `kill_agent` y la consume. No pinta nada. En el simulador se usa `CloseOfficeDialog`.
 */
export function NativeCloseOffice({ store, killAgent }: Props) {
  useEffect(
    () =>
      store.subscribe((s, prev) => {
        const id = s.view.pendingKill;
        if (id === undefined || id === prev.view.pendingKill) return;
        s.cancelKill();
        killAgent(id).catch((e) =>
          console.warn(JSON.stringify({ event: "kill.error", agentId: id, error: e })),
        );
      }),
    [store, killAgent],
  );
  return null;
}
