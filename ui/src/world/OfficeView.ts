// Una oficina: textura compartida + lo dinámico (semáforo, nombre, avatar, herramienta).
// Solo traduce el estado a Pixi; la lógica vive en domain/ y state/.

import { BitmapText, Container, Graphics, Rectangle, Sprite, type Texture } from "pixi.js";
import type { Lod } from "../domain/lod";
import { displayName } from "../domain/names";
import type { AgentState, StatusKind } from "../protocol";
import { anchors, type RoomSize } from "./officeTemplate";
import { PAL, PROVIDER_COLOR, PROVIDER_LETTER, STATUS_COLOR } from "./palette";

export const FONT = "kiti-pixel";
/** Caracteres que caben entre el borde y la ventana con fontSize 9. */
const LABEL_MAX = 15;

const LAMP_FOR: Record<StatusKind, 0 | 1 | 2 | undefined> = {
  error: 0,
  dead: 0,
  waiting: 1,
  working: 2,
  idle: undefined,
};

export class OfficeView {
  readonly root = new Container();
  private readonly border = new Graphics();
  private readonly light = new Graphics();
  private readonly screen = new Graphics();
  private readonly label: BitmapText;
  private readonly near = new Container();
  private readonly badge = new Graphics();
  private readonly badgeText: BitmapText;
  private readonly avatar = new Graphics();
  private readonly bubble = new Graphics();
  private readonly bubbleText: BitmapText;
  private readonly deadShade = new Graphics();
  private readonly selection = new Graphics();
  private agent: AgentState | undefined;

  constructor(
    template: Texture,
    private readonly size: RoomSize,
  ) {
    const a = anchors(size);
    this.root.cullable = true;
    this.root.cullArea = new Rectangle(-4, -4, size.w + 8, size.h + 8);

    this.label = new BitmapText({ text: "", style: { fontFamily: FONT, fontSize: 9 } });
    this.label.position.set(a.label.x, a.label.y);
    this.badgeText = new BitmapText({ text: "", style: { fontFamily: FONT, fontSize: 8 } });
    this.badgeText.position.set(a.badge.x + 3, a.badge.y + 1);
    this.bubbleText = new BitmapText({ text: "", style: { fontFamily: FONT, fontSize: 7 } });
    this.bubbleText.tint = PAL.bubbleText; // las fuentes bitmap se colorean con tint, no con fill
    this.bubbleText.position.set(a.bubble.x + 4, a.bubble.y + 3);

    this.screen.position.set(a.desk.x + size.tile, a.desk.y - 7);
    this.light.position.set(a.light.x, a.light.y);
    this.avatar.position.set(a.avatar.x, a.avatar.y);

    this.near.addChild(this.badge, this.badgeText, this.avatar, this.bubble, this.bubbleText);
    this.root.addChild(
      new Sprite(template),
      this.screen,
      this.near,
      this.deadShade,
      this.border,
      this.light,
      this.label,
      this.selection,
    );
  }

  update(agent: AgentState): void {
    const prev = this.agent;
    this.agent = agent;
    const kind = agent.status.kind;
    if (prev?.status.kind !== kind) this.drawStatus(kind);
    if (prev?.cwd !== agent.cwd) this.label.text = displayName(agent.cwd, LABEL_MAX);
    if (prev?.provider !== agent.provider) this.drawProvider(agent);
    if (prev?.lastTool !== agent.lastTool || prev?.status.kind !== kind) this.drawBubble(agent);
  }

  setLod(lod: Lod): void {
    this.label.visible = lod !== "far";
    this.near.visible = lod === "near";
  }

  setSelected(selected: boolean): void {
    this.selection.clear();
    if (selected) {
      this.selection
        .rect(-6, -6, this.size.w + 12, this.size.h + 12)
        .stroke({ color: PAL.selection, width: 2, alpha: 0.9 });
    }
  }

  private drawStatus(kind: StatusKind): void {
    const { w, h } = this.size;
    const color = STATUS_COLOR[kind];

    this.border
      .clear()
      .rect(-3, -3, w + 6, h + 6)
      .stroke({ color, width: 3 });

    // Semáforo: carcasa con tres lámparas (rojo, ámbar, verde); idle las deja todas apagadas.
    const lit = LAMP_FOR[kind];
    const lamps = [STATUS_COLOR.error, STATUS_COLOR.waiting, STATUS_COLOR.working];
    this.light.clear().roundRect(-1, 0, 12, 26, 2).fill(PAL.housing);
    lamps.forEach((c, i) => {
      const on = lit === i;
      if (on) this.light.circle(5, 5 + i * 8, 5).fill({ color: c, alpha: 0.35 });
      this.light.circle(5, 5 + i * 8, 3).fill(on ? c : PAL.lampOff);
    });

    // Pantalla del monitor con el color del estado.
    this.screen
      .clear()
      .rect(0, 0, this.size.tile, 7)
      .fill(kind === "dead" ? PAL.monitor : color);

    this.deadShade.clear();
    if (kind === "dead") {
      this.deadShade.rect(0, 0, w, h).fill({ color: 0x000000, alpha: 0.45 });
      const cx = w / 2;
      const cy = h / 2 + 8;
      this.deadShade.moveTo(cx - 14, cy - 14).lineTo(cx + 14, cy + 14);
      this.deadShade.moveTo(cx + 14, cy - 14).lineTo(cx - 14, cy + 14);
      this.deadShade.stroke({ color: STATUS_COLOR.error, width: 4 });
    }
  }

  private drawProvider(agent: AgentState): void {
    const a = anchors(this.size);
    this.badge
      .clear()
      .roundRect(a.badge.x, a.badge.y, 12, 11, 2)
      .fill(PROVIDER_COLOR[agent.provider]);
    this.badgeText.text = PROVIDER_LETTER[agent.provider];

    // Avatar sentado: cabeza, pelo y torso con el color del proveedor.
    this.avatar
      .clear()
      .rect(-5, 2, 10, 9)
      .fill(PROVIDER_COLOR[agent.provider])
      .rect(-4, -7, 8, 8)
      .fill(PAL.skin)
      .rect(-4, -8, 8, 3)
      .fill(PAL.hair)
      .rect(-2, -4, 1, 1)
      .fill(PAL.outline)
      .rect(1, -4, 1, 1)
      .fill(PAL.outline);
  }

  private drawBubble(agent: AgentState): void {
    const a = anchors(this.size);
    const kind = agent.status.kind;
    const text =
      kind === "waiting" ? "! permiso" : kind === "error" ? "error" : (agent.lastTool ?? "");
    const show = text !== "" && kind !== "dead" && kind !== "idle";
    this.bubbleText.text = show ? text.slice(0, 14) : "";
    this.bubble.clear();
    if (!show) return;
    const width = Math.max(20, this.bubbleText.width + 8);
    const fill = kind === "waiting" ? STATUS_COLOR.waiting : PAL.bubble;
    this.bubble
      .roundRect(a.bubble.x, a.bubble.y, width, 13, 3)
      .fill(fill)
      .moveTo(a.bubble.x + 6, a.bubble.y + 13)
      .lineTo(a.bubble.x + 4, a.bubble.y + 18)
      .lineTo(a.bubble.x + 11, a.bubble.y + 13)
      .fill(fill);
  }
}
