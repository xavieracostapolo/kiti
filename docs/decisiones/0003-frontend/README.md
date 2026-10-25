# ADR 0003: Arquitectura del frontend (`ui/`)

- **Fecha:** 2026-10-24
- **Estado:** Aceptada con cambios (ver la revisión al final)
- **Decisión:** Pixi imperativo montado desde un componente React, sin `@pixi/react`. Estado en un store `zustand/vanilla` al que el mundo se suscribe fuera de React. Una interfaz `Transport` con implementaciones `Sim` y `Tauri` (`Replay` más adelante). Cámara propia mínima en lugar de `pixi-viewport`. La UI arranca en un navegador normal con `?sim=N&seed=S`. Biome para lint y formato, TypeScript 7 en modo strict, Vitest + Testing Library + Playwright (WebKit en macOS para lo visual).

## Contexto

El stack está cerrado en el [ADR 0001](../0001-stack/README.md): Tauri v2, PixiJS v8 y React solo para paneles. (`pixi-viewport`, que también fijaba, se sustituye por una cámara propia: ver §5 y la revisión.) Este ADR define cómo se organiza `ui/` para cumplir tres requisitos:

1. **Rendimiento.** Entre 20 y 50 oficinas, diffs a unos 10 Hz y CPU casi nula en reposo.
2. **TDD.** La lógica tiene que poder probarse sin WebGL.
3. **Iteración visual con un agente de IA.** La UI tiene que funcionar en `vite dev`, fuera de Tauri, con agentes simulados y deterministas, para que Claude Code pueda abrirla, hacer capturas y comparar.

## Decisión

### 1. Capas

```
ui/src/
  protocol/   generated/ (ts-rs, no se edita a mano) + index.ts (reexporta)
  transport/  Transport + sim/ + tauri/ (replay/ más adelante)
  domain/     lógica pura: layout, LOD, cámara, tiles, plan de escena
  state/      store zustand (vanilla), applyBatch, selectores
  telemetry/  contadores e histogramas de la UI (sin dependencias)
  world/      Pixi imperativo: World, OfficeView, RenderScheduler, camera/
  panels/     React: App, WorldHost, AgentPanel, CloseOfficeDialog, DebugPanel
  dev/        window.__kiti, ganchos de test y bench (solo en modo sim)
```

Las dependencias van en un solo sentido: `protocol → transport → state → (world | panels)`, y `domain` lo usan todas las capas. Dos reglas se hacen cumplir con `noRestrictedImports` de Biome, configurado por carpeta:

- `domain/`, `state/`, `telemetry/` y `transport/sim/` no importan `pixi.js` ni `react`.
- Solo `world/` importa `pixi.js`, y solo `transport/tauri/` importa `@tauri-apps/api`.

### 2. Interfaz `Transport` (lado TS)

```ts
// transport/Transport.ts — todos los tipos vienen de protocol/ (ts-rs, ADR 0004)
import type { AgentId, Hello, KillResult, ServerMsg, UiMetrics } from "../protocol";

export interface Transport {
  /** "in-app": la UI muestra su AlertDialog (Sim). "native": confirma Rust con un diálogo nativo (Tauri). */
  readonly killConfirmation: "in-app" | "native";
  /** Conecta. El primer mensaje es siempre `Snapshot`; después llegan `Diff` (~10 Hz) y, si se pide, `Metrics`.
   *  Si `seq` tiene un hueco, la UI hace dispose() y connect() de nuevo. */
  connect(onMsg: (msg: ServerMsg) => void): Promise<Hello>;
  /** Termina el proceso del agente. Solo tras la confirmación que corresponda. */
  killAgent(id: AgentId): Promise<KillResult>;
  setDebugStream(on: boolean): Promise<void>;
  /** Resumen cada 2 s. No-op en Sim. */
  reportUiMetrics(m: UiMetrics): void;
  dispose(): void;
}
```

El transporte se elige en `main.ts`:

- Si la URL lleva `?sim=` se usa `SimTransport`.
- (Más adelante) si lleva `?replay=<archivo>` se usará `ReplayTransport`, que reproducirá una sesión grabada.
- Si existe `window.__TAURI_INTERNALS__` se usa `TauriTransport`. Se carga con `import()` dinámico para que `@tauri-apps/api` no entre en el bundle web.
- En un navegador sin parámetros se usa `sim=20&seed=1`.

`SimTransport` usa `pure-rand` (`xoroshiro128plus`, un generador por agente con `seed ^ índice`) y un **reloj virtual**. Cada `step()` avanza 100 ms virtuales y emite como mucho un `Diff`, igual que el coalescedor del core. Genera señales normalizadas (los 8 eventos del ADR 0005) con los perfiles de `fixtures/sim/profiles.json` y las convierte en estados con un mapeo mínimo que se valida contra `fixtures/sim/conformance/*.json`, los mismos archivos que prueban el reducer de Rust. No usa `Date.now()` ni `Math.random()`, así que la misma semilla da siempre el mismo mundo. `killAgent` simula una salida **esperada**: el agente desaparece en el siguiente paso (no pasa a `dead`, que significa muerte inesperada).

### 3. Pixi y React: imperativo, no `@pixi/react`

| Criterio | `@pixi/react` 8.0.5 | Pixi imperativo en `<WorldHost>` |
|---|---|---|
| Render bajo demanda | El reconciler y `useTick` asumen el ticker | Control total: `autoStart: false` |
| 10 Hz × 50 agentes | Cada diff pasa por la reconciliación de React | Mutación directa de los `Container` afectados |
| Cámara | Hay que envolverla con `extend` y un ref | Se integra de forma nativa |
| Testabilidad | La lógica acaba mezclada en JSX | Lógica en `domain/`, render fino |
| Acoplamiento | `react-reconciler` + `its-fine`; último release 2025-12-01 | Ninguno |

**Decisión:** `<WorldHost>` crea un `<div>`, y un `useEffect` llama a `createWorld(el, store, transport)` y destruye el mundo al desmontar. `app.init()` es asíncrono y React 19 en StrictMode monta dos veces, así que `createWorld` devuelve un `dispose` que aborta una inicialización pendiente (hay un test para esto). React nunca recibe datos por frame.

### 4. Estado: zustand vanilla

- Hay un store `createStore` de `zustand/vanilla`. `applyBatch(state, batch)` es un reducer puro que devuelve un `Map<AgentId, AgentView>` nuevo y conserva la referencia de los agentes que no cambian.
- **El mundo** se suscribe con `store.subscribe(next, prev)`, compara referencias por id (O(n), con n ≤ 50) y aplica solo las operaciones de `planScene(prev, next)`: `add`, `remove`, `setStatus` y `setLabel`.
- **Los paneles** usan `useStore(store, selector)` con selectores finos: el agente seleccionado, los contadores por color y la lista ordenada con `useShallow`. Un cambio de semáforo en la oficina 37 no vuelve a renderizar nada de React si esa oficina no está seleccionada.
- El estado de vista (`selectedId`, `pendingKill`, nivel de LOD) vive en el mismo store, en un *slice* aparte.

**Alternativas descartadas:**

- valtio: el proxy y la mutación implícita complican el diff por referencia.
- jotai: su modelo atómico está pensado para React, y el consumidor principal aquí es Pixi.
- Redux Toolkit: añade ceremonia sin beneficio.
- Un emisor de eventos propio: sería reinventar la rueda.

### 5. Patrón del mundo

- **Escena:** `worldRoot` (un `Container` cuya posición y escala pone la cámara) → `officesLayer` (un `Container` por oficina) → `overlayLayer` (selección). Los textos usan `BitmapText` con una fuente pixel, para no generar una textura por cada cambio de texto. Como la CSP de Tauri no permite `'unsafe-eval'`, `main.ts` importa `pixi.js/unsafe-eval` antes de crear la aplicación.
- **Salas:** el layout se basa en *slots* estables. `assignSlots(prev, ids)` da a cada agente nuevo el hueco libre más bajo, y nadie se mueve cuando otro agente muere. `slotRect(slot, opts)` convierte el slot en coordenadas de mundo. Las dos funciones son puras.
- **Plantilla de oficina:** se dibuja en Tiled con los *tilesheets* de Kenney tal cual (rejilla de 16 px) y se exporta como `.tmj`. Los *frames* se calculan con una función pura a partir de los metadatos del tileset (`tilewidth`, `spacing`, `margin`, `columns`), sin herramienta de empaquetado. Se compone una sola vez con sprites y se rasteriza con `renderer.generateTexture()`. Las 50 oficinas comparten esa textura y encima llevan solo lo dinámico: semáforo, nombre y avatar.
- **Cámara propia mínima** (sustituye a `pixi-viewport`, ver Riesgos y revisión). Dos piezas:
  - `domain/camera.ts`, puro y probado con Vitest y fast-check: estado `{x, y, k}`, `fitAll`, `focusTarget(rect)`, `zoomAt(punto, factor)` (el punto bajo el cursor no se mueve), `panBy`, `clamp` a los límites del mundo y a `[kMin, kMax]`, e interpolación de foco con *easing* en función de un tiempo inyectado (400 ms; 0 con `anim=0`).
  - `world/camera/CameraInput.ts`: traduce eventos del DOM a llamadas del dominio. Arrastre con Pointer Events y `setPointerCapture`; rueda o dos dedos en el trackpad = pan; `ctrl`/`cmd` + rueda = zoom (así llega el *pinch* en Chromium y WebView2); `gesturestart`/`gesturechange` = zoom (así llega el *pinch* en WebKit/WKWebView); doble clic = `focus`; teclas `0` (ver todo), `+`/`-` y `Esc` (volver a la vista general). Cada cambio llama a `RenderScheduler.request()`. Sin inercia en el MVP.
- **LOD:** `lodFor(scale, prev)` con histéresis tiene tres niveles. `far`: sala y semáforo. `mid`: además, el nombre. `near`: interior, avatar y última herramienta. Cambiar de nivel solo alterna `visible`.
- **Culling:** `CullerPlugin`, con `cullable = true` y `cullArea` fijo por oficina, porque ya se conoce su rectángulo.
- **Render bajo demanda:** se crea la aplicación con `autoStart: false`. `RenderScheduler.request()` agrupa las peticiones en un solo `requestAnimationFrame`. El ticker solo corre mientras haya animaciones registradas (incluida la del foco de cámara) o un arrastre en curso. Con `visibilitychange` en `hidden`, se pausa. Se fijan `TextureStyle.defaultOptions.scaleMode = 'nearest'` y `roundPixels: true`.

### 6. Paneles

Se usan las primitivas de Radix (paquete `radix-ui`): `AlertDialog` para confirmar el cierre de una oficina (solo si `transport.killConfirmation === "in-app"`; en Tauri confirma el diálogo nativo y no se muestran dos), `Tooltip` y `DropdownMenu`. `ScrollArea` **no** se usa: inyecta un elemento `<style>` que la CSP `style-src 'self'` bloquearía; basta con `overflow: auto` en CSS. Los estilos son CSS Modules con tokens en variables CSS para la estética pixel. No se usa Tailwind ni shadcn: serían otra capa de estilos y código copiado para unos pocos paneles.

**Alternativa válida:** `@base-ui/react` 1.9. Radix gana por recorrido y porque su `AlertDialog` resuelve directamente el caso de confirmar el cierre.

### 7. Lint y formato: Biome

`typescript-eslint` 8.72 declara como dependencia `typescript >=4.8.4 <6.1.0`, así que no funciona con TypeScript 7, porque TS 7 todavía no tiene una API programática estable. Biome 2.6 es un solo binario para lint y formato, no depende de `tsc` y trae reglas de hooks (`useExhaustiveDependencies`, `useHookAtTopLevel`). `noFloatingPromises` existe pero está en *nursery*. El tipado se comprueba con `tsc --noEmit` (TS 7, `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`).

### 8. Paquetes

Datos tomados de registry.npmjs.org y api.npmjs.org el 2026-10-24. "Desc/sem" son las descargas semanales en npm.

| Paquete | Versión | Último release | Desc/sem | Uso | Alternativa descartada y por qué |
|---|---|---|---|---|---|
| vite | 8.3.5 | 2026-10-21 | 127 M | Dev server y build | Ya fijado en el ADR 0001 |
| typescript | 7.0.2 | 2026-07-08 | 196 M | `tsc --noEmit` strict | 6.0.3: solo si una herramienta necesitara la API programática (ver revisión) |
| pixi.js | 8.22.0 (versión exacta) | 2026-10-01 | 884 k | Render del mundo | Fijado en el ADR 0001 |
| react / react-dom | 19.3.1 | 2026-10-14 | 124 M | Paneles | — |
| @vitejs/plugin-react | 6.1.2 | 2026-10-05 | 63 M | JSX y HMR | — |
| zustand | 5.0.16 | 2026-10-22 | 40.5 M | Store | valtio 2.3.2, jotai 3.0.1 (ver §4) |
| radix-ui | 1.8.1 | 2026-10-12 | 11.7 M | Primitivas accesibles | shadcn (Tailwind + código copiado); @base-ui/react 1.9.0 (válida) |
| @biomejs/biome | 2.6.3 | 2026-10-24 | 11.9 M | Lint y formato | ESLint 10.13 + typescript-eslint + Prettier 3.9 (no admiten TS 7) |
| vitest + @vitest/coverage-v8 | 5.0.5 | 2026-10-15 | 83.7 M | Tests unitarios y cobertura | Jest: más lento y con config duplicada respecto a Vite |
| @testing-library/react + user-event | 16.3.3 / 14.6.8 | 2026-08-27 / 2026-10-13 | 45 M / 37 M | Tests de paneles | — |
| jsdom | 30.1.3 | 2026-10-15 | 73 M | Entorno DOM de Vitest | happy-dom 21: menos fiel con el foco y los diálogos |
| @playwright/test | 1.64.1 | 2026-10-14 | 49.8 M | E2E, regresión visual, FPS y capturas | Cypress: su soporte de WebKit es experimental y no tiene CLI de capturas |
| pure-rand | 8.4.2 | 2026-07-10 | 56 M | PRNG con semilla para Sim | seedrandom 3.0.5: último release en 2019 |
| fast-check | 4.10.2 | 2026-09-19 | 35 M | Tests de propiedades del layout | — |
| @tauri-apps/api | 2.12.4 | 2026-10-14 | 2.1 M | Solo en `TauriTransport`; `mocks` en sus tests | — |

**Assets sin empaquetador:** `@assetpack/core` (último release 2025-11-07, 7.4 k descargas a la semana) se descarta para el MVP. Los tilesheets de Kenney ya son una rejilla, Vite ya pone hash a los archivos importados para invalidar la caché y `Assets.load` carga las URLs que devuelve Vite. Se reevalúa si hacen falta varias resoluciones, compresión o empaquetar muchas imágenes sueltas.

**Mapas:** Tiled `.tmj` con un parser propio y tipado del subconjunto que se usa: tileset embebido, capas de tiles y una capa de objetos con anclas (`signal`, `label`, `avatar`, `door`). Se descartan `pixi-tiledmap` 2.11 (547 descargas a la semana y un solo mantenedor) y `@pixi/tilemap` 5.0.2 (está pensado para mapas grandes con scroll, y aquí la plantilla es pequeña y se rasteriza una vez). Si el mundo crece a un mapa abierto, se reevalúa `pixi-tiledmap`.

### 9. Estrategia TDD

| Capa | Herramienta | Qué se prueba |
|---|---|---|
| `domain/`, `state/` | Vitest (entorno `node`) + fast-check | `assignSlots`, `slotRect`, `lodFor`, encuadre de cámara, `applyBatch`, `planScene`, `RenderScheduler` con reloj falso, `SimTransport` (misma semilla, misma secuencia) |
| `panels/` | Vitest (`jsdom`) + Testing Library | El diálogo de cierre llama a `killAgent` solo tras confirmar; los selectores no provocan re-render (contador de renders) |
| `world/` | Playwright contra `vite dev` | Render real con WebGL. jsdom no tiene WebGL, así que Pixi no se prueba en unidad: el código de `world/` se limita a traducir operaciones a llamadas de Pixi |
| Regresión visual | `toHaveScreenshot` en el proyecto `webkit-visual`, **solo en macOS** | `?sim=20&seed=42&steps=120&freeze=1&anim=0` en zoom `far`, `mid` y `near` |
| Rendimiento | Playwright con WebKit en modo headed, **solo en local** | `?sim=50&seed=1&bench=pan` |

**Ejemplo test-first** (rojo → verde → refactor). Primero se escribe el test y falla porque `layout.ts` no existe. Después se implementa lo mínimo para que pase.

```ts
// domain/layout.test.ts
import { describe, expect, it } from "vitest";
import { assignSlots, slotRect } from "./layout";

const opts = { cols: 6, roomW: 12, roomH: 9, gap: 2, tile: 16 } as const;

describe("layout de oficinas", () => {
  it("un agente nuevo ocupa el hueco libre más bajo y nadie se mueve", () => {
    const prev = new Map([["a", 0], ["b", 1], ["c", 2]]);
    const next = assignSlots(prev, ["a", "c", "d"]); // b murió
    expect(next).toEqual(new Map([["a", 0], ["c", 2], ["d", 1]]));
  });

  it("convierte un slot en un rectángulo del mundo", () => {
    // slot 7 → columna 1, fila 1; paso = (12+2)·16 = 224 en x y (9+2)·16 = 176 en y
    expect(slotRect(7, opts)).toEqual({ x: 224, y: 176, w: 192, h: 144 });
  });
});
```

Después se añade una propiedad con fast-check: para cualquier secuencia de altas y bajas, no hay dos rectángulos que se solapen y los ids supervivientes conservan su slot.

**FPS y frame time:** `dev/bench.ts` registra los deltas de `requestAnimationFrame` durante un recorrido de cámara guionizado de 10 s y expone `window.__kiti.bench()`, que devuelve `{p50, p95, p99, frames}`. El test de rendimiento falla en estos casos:

- p95 > 16.7 ms con 50 agentes;
- se renderiza algún frame durante 2 s de reposo sin cambios de estado (`?sim=20&freeze=1`).

Los números de Playwright no sustituyen la medición dentro de Tauri (Web Inspector de Safari e Instruments). Sirven para detectar regresiones.

### 10. Ciclo visual con el agente

En modo sim, la app expone `window.__kiti`, con `ready`, `step(n)`, `focus(id)`, `setZoom(z)` y `bench()`. Cuando la escena queda estable (assets cargados, fuente instalada y sin render pendiente), el contenedor del mundo recibe `data-world-ready="true"`.

Parámetros de URL: `sim`, `seed`, `steps` (avanza el reloj virtual), `freeze` (detiene la simulación), `zoom=far|mid|near`, `focus=<id|índice>` y `anim=0`.

Pasos que sigue el agente:

Los comandos exactos (con `pnpm` y `mise`, ADR 0004) están en el plan del Hito 1 del [ADR 0006](../0006-arquitectura-consolidada/README.md). Resumen: `pnpm -C ui dev --host 127.0.0.1 --port 5173 --strictPort` en segundo plano y después `pnpm -C ui exec playwright screenshot --browser=webkit --viewport-size=1440,900 --wait-for-selector='[data-world-ready="true"]' "<url>" .shots/<nombre>.png`, con `anim=0` en la URL.

El agente lee los PNG de `ui/.shots/` (ignorada por git), cambia el código, HMR recarga y vuelve a capturar. Cuando el usuario da el resultado por bueno, `pnpm -C ui exec playwright test --project=webkit-visual --update-snapshots` fija las baselines de `ui/e2e/__screenshots__/`. La configuración de Playwright usa `webServer` con `pnpm dev` y `reuseExistingServer`, así que `playwright test` también levanta Vite solo.

### 11. Assets

Para el MVP se usan Kenney.nl (CC0): [Roguelike Indoors](https://kenney.nl/assets/roguelike-indoors) para el interior de las oficinas, [Roguelike Characters](https://kenney.nl/assets/roguelike-characters) para los avatares, [1-Bit Pack](https://kenney.nl/assets/1-bit-pack) para los iconos de estado y [Kenney Fonts](https://kenney.nl/assets/kenney-fonts) para el texto. Los packs de LimeZu quedan pendientes de revisar su licencia.

```
ui/
  assets/
    tiles/roguelike-indoors.png     # tilesheet de Kenney tal cual (rejilla de 16 px)
    characters/roguelike-chars.png
    icons/1bit.png
    maps/office.tmj                 # plantilla de Tiled que referencia esos tilesheets
    fonts/kenney-pixel.ttf
    LICENSES.md                     # pack, URL, licencia y fecha de descarga
```

Al arrancar, `Assets.load` carga las URLs con hash que devuelve Vite y la fuente antes de marcar `ready`.

## Consecuencias

- La misma UI funciona en Tauri, en el navegador y en CI, sin cambiar código. Lo único que cambia es el `Transport`.
- La mayor parte de la lógica se prueba en Vitest en milisegundos, y Pixi queda reducido a una capa fina que se verifica visualmente.
- Las baselines visuales dependen del motor y del SO (`webkit-darwin`), así que solo se generan y comparan en macOS. En Ubuntu se ejecutan los E2E funcionales con Chromium, sin capturas (ADR 0004).

## Riesgos

- **Cámara propia:** el *pinch* del trackpad llega distinto en WebKit (`GestureEvent`) y en Chromium (`ctrl` + rueda). Playwright no simula bien los gestos de WebKit, así que hay un test unitario del mapeo y una prueba manual en el Hito 1. Si hace falta inercia, rebote o seguimiento, se reevalúa usar una librería.
- **pixi.js fijado:** se fija a 8.22.0 exacto y Renovate propone las subidas, que pasan por el E2E de humo.
- **Biome:** `noFloatingPromises` todavía está en *nursery*.
- **TypeScript 7 en el editor:** VS Code necesita la extensión oficial de TS 7 (*native preview*). Si un editor o una herramienta necesita la API programática, que no llega hasta TS 7.1, se usa `typescript@6.0.3` solo para esa herramienta.

## Dependencias con otros ADRs

Resueltas en la revisión (ADR 0006):

- **Integración (IPC):** `TauriTransport` usa el command `subscribe` con un `Channel<ServerMsg>`; `protocol/generated/` sale de `crates/protocol` con ts-rs; `Diff { seq, ingestMs, upserts, removed }` y resync con `connect()`; `killAgent` acepta solo un `AgentId` del registro.
- **Backend:** el semáforo lo calcula el core. La UI nunca infiere el estado, solo lo pinta. El simulador TS es la única excepción, porque sustituye al core, y su mapeo se valida contra los fixtures de conformidad compartidos.
- **CI:** el job de macOS ejecuta el proyecto `webkit-visual`; las baselines viven en `ui/e2e/__screenshots__/` y se fijan en el Mac del usuario.

## Cuándo reabrir

- Si `@pixi/react` resuelve el render bajo demanda y los paneles necesitan componer objetos de Pixi.
- Si typescript-eslint admite TS 7 y hace falta alguna regla con tipos que Biome no tenga.
- Si el mundo pasa a ser un mapa abierto, porque entonces habría que reevaluar `pixi-tiledmap` y `@pixi/tilemap`.
- Si aparece una librería de cámara mantenida para Pixi v8 o la cámara necesita inercia, rebote o seguimiento.

## Fuentes

- npm: [vite](https://www.npmjs.com/package/vite), [typescript](https://www.npmjs.com/package/typescript), [pixi.js](https://www.npmjs.com/package/pixi.js), [pixi-viewport](https://www.npmjs.com/package/pixi-viewport), [react](https://www.npmjs.com/package/react), [zustand](https://www.npmjs.com/package/zustand), [@pixi/react](https://www.npmjs.com/package/@pixi/react), [radix-ui](https://www.npmjs.com/package/radix-ui), [@base-ui/react](https://www.npmjs.com/package/@base-ui/react), [@assetpack/core](https://www.npmjs.com/package/@assetpack/core), [free-tex-packer-core](https://www.npmjs.com/package/free-tex-packer-core), [pixi-tiledmap](https://www.npmjs.com/package/pixi-tiledmap), [@pixi/tilemap](https://www.npmjs.com/package/@pixi/tilemap), [@biomejs/biome](https://www.npmjs.com/package/@biomejs/biome), [typescript-eslint](https://www.npmjs.com/package/typescript-eslint) (dependencia de typescript), [vitest](https://www.npmjs.com/package/vitest), [@playwright/test](https://www.npmjs.com/package/@playwright/test), [pure-rand](https://www.npmjs.com/package/pure-rand), [seedrandom](https://www.npmjs.com/package/seedrandom), [fast-check](https://www.npmjs.com/package/fast-check)
- GitHub: [pixijs-userland/pixi-viewport](https://github.com/pixijs-userland/pixi-viewport), [pixijs/pixi-react](https://github.com/pixijs/pixi-react), [riebel/pixi-tiledmap](https://github.com/riebel/pixi-tiledmap), [pixijs/assetpack](https://github.com/pixijs/assetpack)
- Documentación:
  - Pixi: [CullerPlugin](https://pixijs.com/8.x/guides/components/application/culler-plugin)
  - [@pixi/react](https://react.pixijs.io/getting-started/)
  - AssetPack: [integración con Vite](https://pixijs.io/assetpack/docs/guide/getting-started/vite/)
  - Biome: [noFloatingPromises](https://biomejs.dev/linter/rules/no-floating-promises/)
  - Playwright: [snapshots](https://playwright.dev/docs/test-snapshots)
  - Tauri: [mocking](https://v2.tauri.app/develop/tests/mocking/)
  - TypeScript: [progreso de TS 7](https://devblogs.microsoft.com/typescript/progress-on-typescript-7-december-2025)
- Kenney: [licencia CC0](https://kenney.nl/support)

## Revisión del arquitecto líder (2026-10-24)

Estado: **Aceptada con cambios.** Detalle en el [ADR 0006](../0006-arquitectura-consolidada/README.md).

1. **Sin `pixi-viewport`: cámara propia mínima.** Verificado: el issue [#523](https://github.com/pixijs-userland/pixi-viewport/issues/523) sigue abierto y sin comentarios desde 2025-06-26, el último *push* al repo es del 2025-02-03, el último release (6.0.3) del 2024-11-27, y ningún fork tiene actividad ni usuarios relevantes. El issue describe incompatibilidades de tipos y una API de Pixi que ya no existe (`updateLocalTransform is not a function`), no una copia duplicada, así que `resolve.dedupe` no era una mitigación fiable. Fijar Pixi para siempre a una versión antigua contradice "no depender de algo muerto". La cámara son cuatro operaciones cuya matemática ya estaba en `domain/camera.ts`; el resto es traducir eventos del DOM. Pixi sigue haciendo el render, el *hit testing* y el *culling*.
2. **`Transport` unificado con el protocolo del ADR 0004:** `connect(onMsg)` devuelve `Hello` y recibe `ServerMsg` (`Snapshot`, `Diff`, `Metrics`). `killConfirmation` evita dos confirmaciones seguidas en Tauri.
3. **Kill en el simulador:** el agente desaparece (salida esperada), no pasa a `dead`.
4. **Simulador TS justificado** (UI sin backend en el navegador y en CI). Comparte `fixtures/sim/profiles.json` y `fixtures/sim/conformance/` con `SimFleet`.
5. **CSP:** `import "pixi.js/unsafe-eval"` (el módulo existe en `pixi.js` 8.22.0) y sin `ScrollArea` de Radix.
6. **Sin AssetPack en el MVP:** rejillas de Kenney y Tiled, y hashes de Vite.
7. **Dónde corre cada test:** las capturas de referencia solo en macOS con WebKit; en Ubuntu, Vitest y E2E funcionales con Chromium; el rendimiento solo en local.
8. **pnpm en lugar de npm** en todos los comandos.
9. **TypeScript 7.0.2 verificado como estable** (publicado el 2026-07-08 como `latest`; 7.1 está en beta). Vite y Vitest transpilan sin `tsc`, así que no dependen de él. `typescript-eslint` 8.72 sigue declarando `typescript <6.1.0` porque TS 7 no tiene API programática hasta la 7.1. Se mantiene **TS 7 + Biome**.
10. **Corrección:** Cypress sí tiene WebKit, aunque experimental. La decisión por Playwright no cambia.
11. Se añaden las capas `telemetry/` y `DebugPanel` (ADR 0004 §3) y la cobertura con `@vitest/coverage-v8`.

