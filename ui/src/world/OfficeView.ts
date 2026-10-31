// Una oficina: textura compartida (tiles de Kenney, ADR 0013) + lo dinámico (semáforo, nombre,
// persona, herramienta, qué hace).
// Encendida muestra un agente; apagada, una conversación cerrada que se puede retomar (ADR 0009).
// Solo traduce el estado a Pixi; la lógica vive en domain/ y state/.

import { BitmapText, Container, Graphics, Rectangle, Sprite, type Texture } from "pixi.js";
import { boardLines, timeline } from "../domain/board";
import type { Lod } from "../domain/lod";
import { displayName } from "../domain/names";
import { decorFor, personFor } from "../domain/people";
import { charsIn, fitLines, fitWords, fontFor, LINE_H } from "../domain/text";
import type { AgentState, ConversationInfo, GitInfo, Provider, StatusKind } from "../protocol";
import type { OfficeTaskBadge } from "../state/taskLinks";
import { anchors, type RoomSize } from "./officeTemplate";
import { PAL, PROVIDER_COLOR, PROVIDER_LETTER, STATUS_COLOR } from "./palette";

export const FONT = "kiti-pixel";

/** Lo que comparten todas las oficinas: variantes de sala por proveedor y los frames de cada persona. */
export type OfficeArt = {
  templates: Record<Provider, readonly Texture[]>;
  people: readonly (readonly Texture[])[];
};
/**
 * Texto que se adapta al zoom: mide más o menos lo mismo en pantalla, así al acercarse caben más
 * caracteres y más líneas. Título en la pared (sobre placa oscura junto a la insignia) y nota abajo.
 */
const LABEL = { screenPx: 13, min: 4, max: 9, width: 96, height: 22, maxLines: 3 };
const NOTE = { screenPx: 11, min: 3, max: 7, height: 40, maxLines: 4 };
/** Tablero del zoom cercano (ADR 0016): en el suelo, bajo la insignia del proveedor. */
const BOARD = { x: 4, y: 55, width: 58, font: 4, pad: 3, bar: 4 };
/** Cartel de la vista lejana: en la parte baja de la sala para no tapar a la persona ni el escritorio. */
const CARD = { screenPx: 12, min: 9, max: 24, maxLines: 3, pad: 8 };
/** Nombres de carpeta enteros: el corte lo hace `fitLines`. */
const FULL = 200;

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
  private readonly labelPlate = new Graphics();
  private readonly label: BitmapText;
  private readonly near = new Container();
  private readonly badge = new Graphics();
  private readonly badgeText: BitmapText;
  private readonly background: Sprite;
  /** Quien trabaja en la oficina: siempre la misma persona para el mismo agente. */
  private readonly person = new Sprite();
  private personFrames: readonly Texture[] = [];
  private animPhase = 0;
  private readonly bubble = new Graphics();
  private readonly bubbleText: BitmapText;
  private readonly deadShade = new Graphics();
  private readonly hoverRing = new Graphics();
  private readonly selection = new Graphics();
  /** Insignia `>_`: la oficina tiene una terminal de Kiti (ADR 0007). */
  private readonly terminalBadge = new Container();
  /** Alias para los mensajes entre agentes y sobre con los que le esperan (ADR 0017). */
  private readonly mailTag = new Container();
  private readonly mailBg = new Graphics();
  private readonly mailText: BitmapText;
  private mailKey = "";
  private lod: Lod = "far";
  /** Etiqueta de agentes efímeros: «⚙ de claude-1 · implementar» o «⚙ 2» (ADR 0019). */
  private readonly taskTag = new Container();
  private readonly taskBg = new Graphics();
  private readonly taskText: BitmapText;
  private taskBadge: OfficeTaskBadge | undefined;
  private taskKey = "";
  /** Franja inferior: qué hace ahora o el último pedido (ADR 0009). */
  private readonly note = new Container();
  private readonly noteBg = new Graphics();
  private readonly noteText: BitmapText;
  private agent: AgentState | undefined;
  private conversation: ConversationInfo | undefined;
  private labelRaw = "";
  private noteRaw = "";
  private labelSize = LABEL.max;
  private noteSize = NOTE.max;
  private cardSize = CARD.max;
  /** Lejos se ve el cartel; en mid y near, el título de la pared y la nota. */
  private far = true;
  private near_ = false;
  /** Tablero del zoom cercano (ADR 0016): sesión, git y la última hora. */
  private readonly board = new Container();
  private readonly boardBg = new Graphics();
  private readonly boardText: BitmapText;
  private readonly boardTimeline = new Graphics();
  private git: GitInfo | null | undefined;
  private boardStale = true;
  /** Texto oculto que cambió: se recalcula al volver a verse (el zoom continuo no lo paga). */
  private readonly stale = { label: false, note: false, card: false };
  /** Cartel de la vista lejana (Hito 13). */
  private readonly card = new Container();
  private readonly cardBg = new Graphics();
  private readonly cardText: BitmapText;

  constructor(
    private readonly art: OfficeArt,
    private readonly size: RoomSize,
    /** Reloj para la línea de tiempo del tablero. */
    private readonly now: () => number = Date.now,
  ) {
    this.background = new Sprite(art.templates.claude[0]);
    const a = anchors(size);
    this.root.cullable = true;
    this.root.cullArea = new Rectangle(-8, -8, size.w + 20, size.h + 24);

    this.label = new BitmapText({ text: "", style: { fontFamily: FONT, fontSize: 9 } });
    this.label.position.set(a.label.x, a.label.y);
    this.badgeText = new BitmapText({ text: "", style: { fontFamily: FONT, fontSize: 8 } });
    this.badgeText.position.set(a.badge.x + 3.5, a.badge.y + 1.5);
    this.bubbleText = new BitmapText({ text: "", style: { fontFamily: FONT, fontSize: 7 } });
    this.bubbleText.tint = PAL.bubbleText; // las fuentes bitmap se colorean con tint, no con fill
    this.bubbleText.position.set(a.bubble.x + 4, a.bubble.y + 3);
    this.noteText = new BitmapText({ text: "", style: { fontFamily: FONT, fontSize: 7 } });
    this.noteText.position.set(6, size.h - 13);
    this.note.addChild(this.noteBg, this.noteText);
    this.cardText = new BitmapText({ text: "", style: { fontFamily: FONT, fontSize: CARD.max } });
    this.card.addChild(this.cardBg, this.cardText);
    this.card.visible = false;

    this.screen.position.set(a.desk.x + size.tile, a.desk.y - 7);
    this.light.position.set(a.light.x, a.light.y);
    this.person.position.set(a.person.x, a.person.y);
    this.person.visible = false;

    this.boardText = new BitmapText({
      text: "",
      style: { fontFamily: FONT, fontSize: BOARD.font },
    });
    this.board.addChild(this.boardBg, this.boardText, this.boardTimeline);
    this.board.position.set(BOARD.x, BOARD.y);
    this.near.addChild(this.bubble, this.bubbleText, this.board);
    const badgeBg = new Graphics()
      .roundRect(0, 0, 22, 14, 3)
      .fill(PAL.housing)
      .stroke({ color: PAL.text, width: 1 });
    const badgeLabel = new BitmapText({ text: ">_", style: { fontFamily: FONT, fontSize: 9 } });
    badgeLabel.position.set(4, 1);
    this.terminalBadge.addChild(badgeBg, badgeLabel);
    // Debajo de la insignia del proveedor (izquierda), sin tapar el cuadro ni el título.
    this.terminalBadge.position.set(6, size.tile * 2 + 4);
    this.terminalBadge.visible = false;
    this.mailText = new BitmapText({ text: "", style: { fontFamily: FONT, fontSize: 6 } });
    this.mailTag.addChild(this.mailBg, this.mailText);
    this.mailTag.visible = false;
    this.taskText = new BitmapText({ text: "", style: { fontFamily: FONT, fontSize: 6 } });
    this.taskText.position.set(3, 1.5);
    this.taskTag.addChild(this.taskBg, this.taskText);
    this.taskTag.visible = false;
    this.root.addChild(
      this.background,
      this.person,
      this.screen,
      this.near,
      this.deadShade,
      this.border,
      this.light,
      this.labelPlate,
      this.label,
      this.badge,
      this.badgeText,
      this.note,
      this.card,
      this.terminalBadge,
      this.mailTag,
      this.taskTag,
      this.hoverRing,
      this.selection,
    );
  }

  update(agent: AgentState): void {
    const prev = this.agent;
    this.agent = agent;
    const kind = agent.status.kind;
    if (prev?.status.kind !== kind) {
      this.drawStatus(kind);
      if (this.far) this.renderCard();
    }
    if (prev?.cwd !== agent.cwd || prev?.title !== agent.title) {
      this.setLabel(agent.title ?? displayName(agent.cwd, FULL));
    }
    if (prev?.provider !== agent.provider || prev?.id !== agent.id) this.drawProvider(agent);
    if (prev?.lastTool !== agent.lastTool || prev?.status.kind !== kind) this.drawBubble(agent);
    if (prev?.activity !== agent.activity || prev?.lastPrompt !== agent.lastPrompt) {
      // Trabajando: qué hace. Si no: lo último que se le pidió.
      this.drawNote(agent.activity ?? (agent.lastPrompt ? `> ${agent.lastPrompt}` : ""));
    }
    // Animación por eventos (0 frames en reposo): cuando está trabajando y llega un evento nuevo,
    // el personaje alterna de postura y la pantalla muestra actividad de código.
    if (kind === "working" && prev !== undefined && prev.lastEventAt !== agent.lastEventAt) {
      this.stepWorkingAnimation();
    } else if (kind !== "working" && this.animPhase !== 0) {
      this.resetWorkingAnimation(kind);
    }
    if (
      prev?.model !== agent.model ||
      prev?.contextPct !== agent.contextPct ||
      prev?.costCents !== agent.costCents ||
      prev?.history !== agent.history
    ) {
      this.refreshBoard();
    }
  }

  /** Texto del tablero si se ve (tests E2E): `undefined` fuera del zoom cercano. */
  boardShown(): string | undefined {
    return this.near_ && this.boardText.text !== "" ? this.boardText.text : undefined;
  }

  /** Estado de git del proyecto de esta oficina (ADR 0016). */
  setGit(git: GitInfo | null | undefined): void {
    if (git === this.git) return;
    this.git = git;
    this.refreshBoard();
  }

  /** Solo se dibuja si se ve (zoom cercano); si no, se marca para cuando se vea. */
  private refreshBoard(): void {
    if (!this.near_) {
      this.boardStale = true;
      return;
    }
    this.boardStale = false;
    this.renderBoard();
  }

  private renderBoard(): void {
    const agent = this.agent;
    this.boardBg.clear();
    this.boardTimeline.clear();
    this.boardText.text = "";
    if (!agent || this.conversation) return;
    const lines = boardLines(agent, this.git);
    const lineH = BOARD.font * LINE_H;
    const textH = lines.length * lineH;
    this.boardText.text = lines.join("\n");
    this.boardText.position.set(BOARD.pad, BOARD.pad);
    const boxW = Math.max(BOARD.width, Math.ceil(this.boardText.width) + 2 * BOARD.pad);
    // Última hora: tramos del color del semáforo; el gris de "inactivo" se atenúa.
    const barY = BOARD.pad + textH + (lines.length ? 2 : 0);
    const barW = boxW - 2 * BOARD.pad;
    this.boardTimeline.rect(BOARD.pad, barY, barW, BOARD.bar).fill({ color: PAL.lampOff });
    for (const seg of timeline(agent.history, this.now())) {
      const x = BOARD.pad + seg.from * barW;
      const w = Math.max(0.5, (seg.to - seg.from) * barW);
      this.boardTimeline
        .rect(x, barY, w, BOARD.bar)
        .fill({ color: STATUS_COLOR[seg.kind], alpha: seg.kind === "idle" ? 0.45 : 1 });
    }
    const height = barY + BOARD.bar + BOARD.pad;
    this.boardBg
      .roundRect(2, 3, boxW, height, 3)
      .fill({ color: 0x000000, alpha: 0.45 })
      .roundRect(0, 0, boxW, height, 3)
      .fill({ color: PAL.housing, alpha: 0.9 })
      .stroke({ color: PAL.zoneBorder, width: 1, alpha: 0.75 });
  }

  /** Avanza un paso de animación del personaje y el monitor al recibir actividad real/simulada. */
  private stepWorkingAnimation(): void {
    this.animPhase = (this.animPhase + 1) % 3;
    const a = anchors(this.size);
    if (this.personFrames.length > 0) {
      const f = this.personFrames[this.animPhase % this.personFrames.length];
      if (f) this.person.texture = f;
    }
    this.person.position.set(a.person.x, a.person.y - (this.animPhase % 2));
    this.drawMonitorScreen("working", this.animPhase);
  }

  private resetWorkingAnimation(kind: StatusKind): void {
    this.animPhase = 0;
    const a = anchors(this.size);
    if (this.personFrames[0]) this.person.texture = this.personFrames[0];
    this.person.position.set(a.person.x, a.person.y);
    this.drawMonitorScreen(kind, 0);
  }

  /** Oficina apagada: una conversación cerrada. Sin semáforo ni avatar; clic para retomarla. */
  showConversation(c: ConversationInfo): void {
    const prev = this.conversation;
    this.conversation = c;
    if (prev === undefined) this.drawDormant(c);
    if (prev?.title !== c.title || prev?.cwd !== c.cwd) {
      this.setLabel(c.title ?? displayName(c.cwd, FULL));
    }
    if (prev?.lastPrompt !== c.lastPrompt) this.drawNote(c.lastPrompt ? `> ${c.lastPrompt}` : "");
  }

  /** Escala de la cámara: recalcula el texto solo si cambia su tamaño cuantizado. */
  setTextScale(k: number): void {
    const label = fontFor(k, LABEL.screenPx, LABEL.min, LABEL.max);
    const note = fontFor(k, NOTE.screenPx, NOTE.min, NOTE.max);
    const card = fontFor(k, CARD.screenPx, CARD.min, CARD.max);
    if (label !== this.labelSize) {
      this.labelSize = label;
      this.refresh("label");
    }
    if (note !== this.noteSize) {
      this.noteSize = note;
      this.refresh("note");
    }
    if (card !== this.cardSize) {
      this.cardSize = card;
      this.refresh("card");
    }
  }

  /** Recalcula un texto ahora si se ve; si no, lo marca para cuando vuelva a verse. */
  private refresh(which: "label" | "note" | "card"): void {
    const visible = which === "card" ? this.far : !this.far;
    if (!visible) {
      this.stale[which] = true;
      return;
    }
    this.stale[which] = false;
    if (which === "label") this.renderLabel();
    else if (which === "note") this.renderNote();
    else this.renderCard();
  }

  /** Atenuada: no coincide con el filtro. */
  setDimmed(dimmed: boolean): void {
    this.root.alpha = dimmed ? 0.22 : 1;
  }

  setLod(lod: Lod): void {
    this.far = lod === "far";
    this.card.visible = this.far;
    this.labelPlate.visible = !this.far;
    this.label.visible = !this.far;
    this.note.visible = !this.far;
    this.near.visible = lod === "near";
    this.near_ = lod === "near";
    this.lod = lod;
    this.drawTaskTag();
    if (this.near_ && this.boardStale) this.refreshBoard();
    for (const which of ["label", "note", "card"] as const) {
      if (this.stale[which]) this.refresh(which);
    }
  }

  /**
   * Alias del agente para los mensajes (`codex-1`) y, si hay mensajes esperándole, un sobre con
   * cuántos (ADR 0017). Va a la derecha, bajo el semáforo, como la insignia `>_` a la izquierda.
   */
  setMail(alias: string | undefined, waiting: number): void {
    const key = `${alias ?? ""}|${waiting}`;
    if (key === this.mailKey) return;
    this.mailKey = key;
    this.mailTag.visible = alias !== undefined;
    if (alias === undefined) return;
    const env = waiting > 0 ? 11 : 0;
    this.mailText.text = waiting > 0 ? `${waiting} ${alias}` : alias;
    this.mailText.tint = waiting > 0 ? PAL.mailInk : PAL.text;
    this.mailText.position.set(3 + env, 1.5);
    const w = Math.ceil(this.mailText.width) + 6 + env;
    this.mailBg
      .clear()
      .roundRect(0, 0, w, 10, 2)
      .fill(waiting > 0 ? PAL.mailFill : PAL.housing)
      .stroke({ color: waiting > 0 ? PAL.mailInk : PAL.text, width: 1 });
    if (waiting > 0) {
      // Sobre: rectángulo con la solapa en V.
      this.mailBg
        .rect(3, 2, 8.5, 6)
        .fill(PAL.envelope)
        .stroke({ color: PAL.mailInk, width: 0.75 })
        .moveTo(3, 2)
        .lineTo(7.25, 5.5)
        .lineTo(11.5, 2)
        .stroke({ color: PAL.mailInk, width: 0.75 });
    }
    this.mailTag.position.set(this.size.w - w - 6, this.size.tile * 2 + 4);
  }

  /** Quién lanzó esta oficina y cuántos agentes efímeros vivos lanzó ella (ADR 0019). */
  setTaskBadge(badge: OfficeTaskBadge | undefined): void {
    this.taskBadge = badge;
    this.drawTaskTag();
  }

  private drawTaskTag(): void {
    const b = this.taskBadge;
    const full = this.lod !== "far";
    const parts: string[] = [];
    if (b?.by !== undefined) parts.push(full ? `⚙ de ${b.by} · ${b.role ?? ""}` : "⚙");
    if (b && b.launched > 0) parts.push(`⚙ ${b.launched}`);
    const text = parts.join("  ");
    if (text === this.taskKey) return;
    this.taskKey = text;
    this.taskTag.visible = text !== "";
    if (text === "") return;
    this.taskText.text = text;
    this.taskText.tint = PAL.text;
    const w = Math.ceil(this.taskText.width) + 6;
    this.taskBg
      .clear()
      .roundRect(0, 0, w, 10, 2)
      .fill(PAL.housing)
      .stroke({ color: PAL.taskLine, width: 1 });
    this.taskTag.position.set(this.size.w - w - 6, this.size.tile * 2 + 16);
  }

  /** Texto de la etiqueta de agentes efímeros (tests E2E). */
  taskShown(): string | undefined {
    return this.taskTag.visible ? this.taskText.text : undefined;
  }

  /** Texto del alias y del sobre (tests E2E). */
  mailShown(): string | undefined {
    return this.mailTag.visible ? this.mailText.text : undefined;
  }

  setHasTerminal(has: boolean): void {
    this.terminalBadge.visible = has;
  }

  setHovered(hovered: boolean): void {
    this.hoverRing.clear();
    if (hovered) {
      this.hoverRing
        .roundRect(-5, -5, this.size.w + 10, this.size.h + 10, 3)
        .stroke({ color: PAL.selection, width: 1.5, alpha: 0.55 });
    }
  }

  setSelected(selected: boolean): void {
    this.selection.clear();
    if (selected) {
      this.selection
        .roundRect(-7, -7, this.size.w + 14, this.size.h + 14, 4)
        .stroke({ color: PAL.selection, width: 2.5, alpha: 0.95 });
    }
  }

  private setLabel(text: string): void {
    this.labelRaw = text;
    this.refresh("label");
    this.refresh("card");
  }

  private renderCard(): void {
    const width = this.size.w - 2 * CARD.pad - 8;
    const size = fitWords(this.labelRaw, width, this.cardSize, CARD.min);
    const text = fitLines(this.labelRaw, charsIn(width, size), CARD.maxLines);
    this.cardText.style.fontSize = size;
    this.cardText.text = text;
    this.cardBg.clear();
    if (text === "") return;
    const lines = text.split("\n").length;
    const height = Math.ceil(lines * size * LINE_H) + 8;
    // En la mitad inferior de la sala para dejar visible al personaje y su escritorio desde lejos.
    const top = Math.max(Math.round(this.size.h * 0.52), this.size.h - height - 6);
    this.cardText.position.set(CARD.pad + 4, top + 4);
    const strokeColor = this.agent ? STATUS_COLOR[this.agent.status.kind] : PAL.dormant;
    this.cardBg
      .roundRect(CARD.pad + 2, top + 3, this.size.w - 2 * CARD.pad, height, 4)
      .fill({ color: 0x000000, alpha: 0.45 })
      .roundRect(CARD.pad, top, this.size.w - 2 * CARD.pad, height, 4)
      .fill({ color: PAL.housing, alpha: 0.9 })
      .stroke({ color: strokeColor, width: 1.2, alpha: 0.75 });
  }

  private renderLabel(): void {
    const size = this.labelSize;
    const lines = Math.min(LABEL.maxLines, Math.max(1, Math.floor(LABEL.height / (size * LINE_H))));
    const text = fitLines(this.labelRaw, charsIn(LABEL.width, size), lines);
    this.label.style.fontSize = size;
    this.label.text = text;
    this.labelPlate.clear();
    if (text === "") return;
    const a = anchors(this.size);
    const lineCount = text.split("\n").length;
    const h = Math.ceil(lineCount * size * LINE_H) + 4;
    const w = Math.min(LABEL.width + 6, Math.max(36, Math.ceil(this.label.width) + 8));
    this.labelPlate
      .roundRect(a.label.x - 3, a.label.y - 2, w, h, 3)
      .fill({ color: PAL.housing, alpha: 0.78 });
  }

  private drawNote(text: string): void {
    this.noteRaw = text;
    this.refresh("note");
  }

  private renderNote(): void {
    this.noteBg.clear();
    const size = this.noteSize;
    // Con la letra a tamaño completo (lejos), una línea; al acercarse, hasta cuatro.
    const maxLines =
      size >= NOTE.max ? 1 : Math.min(NOTE.maxLines, Math.floor(NOTE.height / (size * LINE_H)));
    const width = this.size.w - 12;
    const text = fitLines(this.noteRaw, charsIn(width, size), maxLines);
    this.noteText.style.fontSize = size;
    this.noteText.text = text;
    if (text === "") return;
    const lines = text.split("\n").length;
    const height = Math.ceil(lines * size * LINE_H) + 4;
    const top = this.size.h - 3 - height;
    this.noteText.position.set(6, top + 2);
    this.noteBg
      .roundRect(3, top, this.size.w - 6, height, 3)
      .fill({ color: PAL.housing, alpha: 0.86 });
  }

  private drawDormant(c: ConversationInfo): void {
    const { w, h } = this.size;
    // Maqueta 2.5D apagada: sombra sobre la zona y frente extruido oscuro.
    this.border
      .clear()
      .roundRect(2, 5, w + 8, h + 11, 4)
      .fill({ color: 0x000000, alpha: 0.42 })
      .rect(-3, h + 2, w + 6, 6)
      .fill(0x272c3a)
      .rect(-3, -3, w + 6, h + 6)
      .stroke({ color: PAL.dormant, width: 2 });
    this.light
      .clear()
      .roundRect(1, 2, 12, 26, 2)
      .fill({ color: 0x000000, alpha: 0.35 })
      .roundRect(-1, 0, 12, 26, 2)
      .fill(PAL.housing);
    for (let i = 0; i < 3; i++) this.light.circle(5, 5 + i * 8, 3).fill(PAL.lampOff);
    this.screen.clear().rect(0, 0, this.size.tile, 7).fill(PAL.monitor);
    // Luz apagada: la sala en penumbra, sin nadie en la silla.
    this.deadShade.clear().rect(0, 0, w, h).fill({ color: 0x000000, alpha: 0.5 });
    const a = anchors(this.size);
    this.badge
      .clear()
      .roundRect(a.badge.x + 1, a.badge.y + 2, 13, 12, 2)
      .fill({ color: 0x000000, alpha: 0.4 })
      .roundRect(a.badge.x, a.badge.y, 13, 12, 2)
      .fill(PROVIDER_COLOR[c.provider])
      .stroke({ color: PAL.housing, width: 1 });
    this.badgeText.text = PROVIDER_LETTER[c.provider];
    const templates = this.art.templates[c.provider];
    this.background.texture = templates[decorFor(c.sessionId, templates.length)] as Texture;
    this.person.visible = false;
    this.bubbleText.text = "zZ";
    this.bubble
      .clear()
      .roundRect(a.bubble.x, a.bubble.y, 20, 13, 3)
      .fill({ color: PAL.bubble, alpha: 0.6 });
  }

  private drawMonitorScreen(kind: StatusKind, phase: number): void {
    const color = STATUS_COLOR[kind];
    const t = this.size.tile;
    this.screen.clear();
    if (kind !== "dead" && kind !== "idle") {
      // Cono de luz 2.5D que el monitor proyecta sobre el escritorio y la persona.
      this.screen
        .poly([-10, 7, t + 10, 7, t + 20, 32, -20, 32])
        .fill({ color, alpha: 0.13 })
        .ellipse(t / 2, 4, t * 0.95, 8)
        .fill({ color, alpha: 0.22 });
    }
    this.screen.rect(0, 0, t, 7).fill(kind === "dead" ? PAL.monitor : color);
    if (kind === "working") {
      // Líneas de código simuladas que avanzan con cada evento del agente.
      const w1 = phase === 0 ? 10 : phase === 1 ? 6 : 12;
      const w2 = phase === 0 ? 6 : phase === 1 ? 11 : 8;
      this.screen
        .rect(2, 1.5, w1, 1.5)
        .fill({ color: PAL.housing, alpha: 0.55 })
        .rect(2, 4, w2, 1.5)
        .fill({ color: PAL.housing, alpha: 0.55 });
    }
  }

  private drawStatus(kind: StatusKind): void {
    const { w, h } = this.size;
    const color = STATUS_COLOR[kind];
    // Versión oscurecida del color del estado para el frente extruido 2.5D de la sala.
    const r = Math.floor(((color >> 16) & 0xff) * 0.52);
    const g = Math.floor(((color >> 8) & 0xff) * 0.52);
    const b = Math.floor((color & 0xff) * 0.52);
    const wallFront = (r << 16) | (g << 8) | b;

    this.border
      .clear()
      // Sombra proyectada de la oficina sobre el suelo de la zona.
      .roundRect(3, 6, w + 8, h + 12, 5)
      .fill({ color: 0x000000, alpha: 0.45 });
    if (kind === "working" || kind === "waiting" || kind === "error") {
      // Resplandor ambiental exterior en oficinas que trabajan o piden atención.
      this.border.roundRect(-6, -6, w + 12, h + 17, 5).stroke({ color, width: 2, alpha: 0.22 });
    }
    this.border
      // Frente inferior extruido (efecto bloque/diorama 2.5D).
      .rect(-3, h + 2, w + 6, 7)
      .fill(wallFront)
      .rect(-3, h + 8, w + 6, 1.5)
      .fill({ color: 0x000000, alpha: 0.45 })
      // Marco principal y bisel superior iluminado.
      .rect(-3, -3, w + 6, h + 6)
      .stroke({ color, width: 3 })
      .rect(-2, -3, w + 4, 1)
      .fill({ color: 0xffffff, alpha: 0.28 });

    // Semáforo con relieve 2.5D: sombra sobre la pared y doble halo en la lámpara activa.
    const lit = LAMP_FOR[kind];
    const lamps = [STATUS_COLOR.error, STATUS_COLOR.waiting, STATUS_COLOR.working];
    this.light
      .clear()
      .roundRect(1, 2, 12, 26, 2)
      .fill({ color: 0x000000, alpha: 0.4 })
      .roundRect(-1, 0, 12, 26, 2)
      .fill(PAL.housing);
    lamps.forEach((c, i) => {
      const on = lit === i;
      if (on) {
        this.light.circle(5, 5 + i * 8, 7.5).fill({ color: c, alpha: 0.18 });
        this.light.circle(5, 5 + i * 8, 5).fill({ color: c, alpha: 0.38 });
      }
      this.light.circle(5, 5 + i * 8, 3).fill(on ? c : PAL.lampOff);
    });

    // Pantalla del monitor con el color del estado.
    this.drawMonitorScreen(kind, this.animPhase);

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
      .roundRect(a.badge.x, a.badge.y, 13, 12, 2)
      .fill(PROVIDER_COLOR[agent.provider])
      .stroke({ color: PAL.housing, width: 1 });
    this.badgeText.text = PROVIDER_LETTER[agent.provider];
    // La alfombra y la variante de muebles se eligen por proveedor e id del agente.
    const templates = this.art.templates[agent.provider];
    this.background.texture = templates[decorFor(agent.id, templates.length)] as Texture;
    const people = this.art.people;
    this.personFrames = people[personFor(agent.id, people.length)] ?? [];
    if (this.personFrames[0]) this.person.texture = this.personFrames[0];
    this.person.visible = true;
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
