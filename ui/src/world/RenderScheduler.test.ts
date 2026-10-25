import { describe, expect, it } from "vitest";
import { RenderScheduler } from "./RenderScheduler";

function fakeRaf() {
  const queue = new Map<number, (t: number) => void>();
  let next = 1;
  return {
    raf: (cb: (t: number) => void) => {
      queue.set(next, cb);
      return next++;
    },
    caf: (h: number) => {
      queue.delete(h);
    },
    /** Ejecuta los callbacks pendientes como un frame del navegador. */
    flush(t: number) {
      const cbs = [...queue.values()];
      queue.clear();
      for (const cb of cbs) cb(t);
    },
    get pending() {
      return queue.size;
    },
  };
}

describe("RenderScheduler", () => {
  it("agrupa varias peticiones en un solo frame", () => {
    const f = fakeRaf();
    let renders = 0;
    const s = new RenderScheduler({ render: () => renders++, raf: f.raf, caf: f.caf });
    s.request();
    s.request();
    s.request();
    expect(f.pending).toBe(1);
    f.flush(16);
    expect(renders).toBe(1);
    expect(s.frames).toBe(1);
  });

  it("sin peticiones ni animaciones se detiene: 0 frames en reposo", () => {
    const f = fakeRaf();
    const s = new RenderScheduler({ render: () => {}, raf: f.raf, caf: f.caf });
    s.request();
    f.flush(16);
    f.flush(32);
    f.flush(48);
    expect(s.frames).toBe(1);
    expect(s.running).toBe(false);
  });

  it("una animación mantiene el bucle hasta que termina", () => {
    const f = fakeRaf();
    const s = new RenderScheduler({ render: () => {}, raf: f.raf, caf: f.caf });
    let left = 3;
    s.animate(() => --left <= 0);
    expect(s.running).toBe(true);
    for (let t = 16; t <= 160; t += 16) f.flush(t);
    expect(s.frames).toBe(3);
    expect(s.running).toBe(false);
  });

  it("pausado no programa frames y al reanudar pinta una vez", () => {
    const f = fakeRaf();
    const s = new RenderScheduler({ render: () => {}, raf: f.raf, caf: f.caf });
    s.pause();
    s.request();
    expect(f.pending).toBe(0);
    s.resume();
    f.flush(16);
    expect(s.frames).toBe(1);
  });

  it("informa del delta entre frames consecutivos", () => {
    const f = fakeRaf();
    const deltas: number[] = [];
    const s = new RenderScheduler({
      render: () => {},
      raf: f.raf,
      caf: f.caf,
      onFrame: (_ms, delta) => {
        if (delta !== undefined) deltas.push(delta);
      },
    });
    s.animate(() => false);
    f.flush(100);
    f.flush(116);
    f.flush(133);
    expect(deltas).toEqual([16, 17]);
  });

  it("dispose cancela el frame pendiente", () => {
    const f = fakeRaf();
    const s = new RenderScheduler({ render: () => {}, raf: f.raf, caf: f.caf });
    s.request();
    s.dispose();
    expect(f.pending).toBe(0);
  });
});
