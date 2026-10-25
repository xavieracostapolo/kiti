// Render bajo demanda (ADR 0003 §5): sin ticker continuo. Se pinta solo cuando algo lo pide
// y mientras haya animaciones registradas. No importa Pixi: recibe la función de render.

export type Animation = (now: number) => boolean; // true = terminó

export type SchedulerOptions = {
  render: (now: number) => void;
  raf?: (cb: (t: number) => void) => number;
  caf?: (handle: number) => void;
  /** Duración del render y delta desde el frame anterior (undefined tras una pausa del bucle). */
  onFrame?: (renderMs: number, rafDelta: number | undefined) => void;
  clock?: () => number;
};

export class RenderScheduler {
  private readonly opts: Required<Omit<SchedulerOptions, "onFrame">> &
    Pick<SchedulerOptions, "onFrame">;
  private readonly animations = new Set<Animation>();
  private handle: number | undefined;
  private lastFrameAt: number | undefined;
  private paused = false;
  frames = 0;

  constructor(opts: SchedulerOptions) {
    this.opts = {
      raf: (cb) => requestAnimationFrame(cb),
      caf: (h) => cancelAnimationFrame(h),
      clock: () => performance.now(),
      ...opts,
    };
  }

  get running(): boolean {
    return this.handle !== undefined || this.animations.size > 0;
  }

  request(): void {
    if (this.paused || this.handle !== undefined) return;
    this.handle = this.opts.raf((t) => this.frame(t));
  }

  animate(step: Animation): void {
    this.animations.add(step);
    this.request();
  }

  pause(): void {
    this.paused = true;
    if (this.handle !== undefined) this.opts.caf(this.handle);
    this.handle = undefined;
    this.lastFrameAt = undefined;
  }

  resume(): void {
    this.paused = false;
    this.request();
  }

  dispose(): void {
    this.pause();
    this.animations.clear();
  }

  private frame(now: number): void {
    this.handle = undefined;
    for (const step of this.animations) if (step(now)) this.animations.delete(step);
    const t0 = this.opts.clock();
    this.opts.render(now);
    const renderMs = this.opts.clock() - t0;
    this.frames++;
    this.opts.onFrame?.(
      renderMs,
      this.lastFrameAt === undefined ? undefined : now - this.lastFrameAt,
    );
    if (this.animations.size > 0) {
      this.lastFrameAt = now;
      this.request();
    } else {
      this.lastFrameAt = undefined;
    }
  }
}
