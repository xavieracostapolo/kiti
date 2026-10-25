import { AlertDialog } from "radix-ui";
import { useStore } from "zustand";
import { displayName } from "../domain/names";
import type { AgentId, KillResult } from "../protocol";
import type { KitiStore } from "../state/store";
import s from "./panels.module.css";

type Props = { store: KitiStore; killAgent: (id: AgentId) => Promise<KillResult> };

/** Confirmación in-app (Sim). En Tauri confirma el diálogo nativo y este no se monta. */
export function CloseOfficeDialog({ store, killAgent }: Props) {
  const pending = useStore(store, (st) =>
    st.view.pendingKill ? st.agents.agents.get(st.view.pendingKill) : undefined,
  );
  const cancel = () => store.getState().cancelKill();
  const confirm = () => {
    if (!pending) return;
    store.getState().cancelKill();
    void killAgent(pending.id);
  };

  return (
    <AlertDialog.Root open={pending !== undefined} onOpenChange={(open) => !open && cancel()}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay className={s.overlay} />
        <AlertDialog.Content className={s.dialog}>
          <AlertDialog.Title className={s.dialogTitle}>¿Cerrar la oficina?</AlertDialog.Title>
          <AlertDialog.Description className={s.dialogText}>
            {pending
              ? `Se terminará el agente ${pending.provider} de «${displayName(pending.cwd)}» (PID ${pending.pid}). Si está editando archivos, puede quedar a medias.`
              : ""}
          </AlertDialog.Description>
          <div className={s.dialogActions}>
            <AlertDialog.Cancel className={s.button}>Cancelar</AlertDialog.Cancel>
            <AlertDialog.Action className={`${s.button} ${s.danger}`} onClick={confirm}>
              Cerrar oficina
            </AlertDialog.Action>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
