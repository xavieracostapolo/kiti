// Conversaciones cerradas falsas para el modo sim (`?conversations=1`): permiten ver y probar las
// oficinas apagadas y "Retomar" sin Tauri (ADR 0009).
import profilesJson from "../../../../fixtures/sim/profiles.json";
import type { ConversationInfo, TerminalInfo } from "../../protocol";
import type { ConversationApi } from "../Transport";

const HOUR = 3_600_000;

/** Dos en el proyecto de `first` y dos en uno que no tiene agentes encendidos. */
export function fakeConversations(project: string, now: number): ConversationInfo[] {
  const other = "/Users/dev/tienda-web";
  const conv = (
    n: number,
    where: string,
    title: string | undefined,
    lastPrompt: string,
  ): ConversationInfo => ({
    sessionId: `0000000${n}-sim0-4000-8000-00000000000${n}`,
    provider: "claude",
    cwd: where,
    project: where,
    ...(title === undefined ? {} : { title }),
    lastPrompt,
    updatedAt: now - n * 5 * HOUR,
  });
  return [
    conv(1, other, "Revisar vulnerabilidades", "revisa las dependencias con CVE abiertos"),
    conv(2, other, "Migración de datos", "migra la tabla de usuarios al nuevo esquema"),
    conv(3, project, profilesJson.titles[5], "documenta los endpoints nuevos"),
    conv(4, project, undefined, "sigue con lo de ayer"),
  ];
}

export class FakeConversations implements ConversationApi {
  private list_: ConversationInfo[];

  /** `resumeAs`: terminal que "aparece" al retomar (en el simulador no se crea un proceso). */
  constructor(
    list: ConversationInfo[],
    private readonly resumeAs: TerminalInfo,
  ) {
    this.list_ = list;
  }

  async list(): Promise<ConversationInfo[]> {
    return [...this.list_];
  }

  async hide(sessionId: string): Promise<void> {
    this.list_ = this.list_.filter((c) => c.sessionId !== sessionId);
  }

  async resume(sessionId: string): Promise<TerminalInfo> {
    if (!this.list_.some((c) => c.sessionId === sessionId)) {
      throw { code: "CONVERSATION_NOT_FOUND", message: "Esa conversación ya no está en la lista" };
    }
    this.list_ = this.list_.filter((c) => c.sessionId !== sessionId);
    return this.resumeAs;
  }
}
