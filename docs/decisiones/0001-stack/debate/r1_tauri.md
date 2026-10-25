# R1 – Apertura: Tauri v2 + TypeScript + PixiJS (+ pixi-viewport)

## Tesis
"Agent Office" es una herramienta de sistema que tiene cara de juego. El 20 % del peso (procesos, señales, sockets, watch) cae en un backend Rust nativo. El 30 % (juego 2D + UI de herramienta) cae en un webview con PixiJS y HTML/CSS. Tauri es el único candidato que pone cada mitad en el runtime que mejor la resuelve sin cargar con un Chromium propio.

## Estado verificado (octubre 2026)
- **Tauri:** el core `tauri` va por la v2.12.5 (21-oct) y la CLI por la 2.12.4. Hay una v3 en alpha que trae un runtime CEF (Chromium) opcional ([releases](https://github.com/tauri-apps/tauri/releases), [digest](https://buttondown.com/thewang/archive/rust-daily-digest-2026-05-24/)). Tiene unas 112k estrellas y es un programa de la Commons Conservancy ([repo](https://github.com/tauri-apps/tauri)).
- **PixiJS:** la v8 está en 8.19/8.20, supera las 500k descargas semanales en npm y publica "agent skills" oficiales para asistentes de IA ([blog jun-2026](https://pixijs.com/blog/june-2026)). La 8.16 recuperó un renderer Canvas 2D que se activa solo si faltan WebGL y WebGPU ([8.16](https://pixijs.com/blog/8.16.0)). pixi-viewport v6 soporta Pixi v8 ([repo](https://github.com/pixi-viewport/pixi-viewport)).

## Puntuación por criterio

| Criterio (peso) | Nota | Justificación |
|---|---|---|
| Integración sistema (20 %) | **9** | Para procesos, `sysinfo` 0.39 da `cmd()`, `cwd()`, `start_time()` y `kill_with(Signal)` en los 3 SO ([docs.rs](https://docs.rs/sysinfo/latest/sysinfo/struct.Process.html)). Para los hooks, un servidor axum en 127.0.0.1 corre sobre el runtime tokio que Tauri ya trae. Para los transcripts, el plugin fs tiene `watch` y `watchImmediate` ([fs](https://v2.tauri.app/plugin/file-system/)), o se usa `notify` directamente. Los eventos llegan a la UI con `emit`. No es un 10 porque Windows no tiene SIGTERM real y hace falta un camino propio (TerminateProcess o envío de Ctrl-Break). |
| Juego 2D (15 %) | **7** | PixiJS es un renderer 2D maduro (sprites, AnimatedSprite, batching, WebGPU). pixi-viewport resuelve pan, zoom, pinch y clamp. Para los mapas, Tiled + `@pixi/tilemap`. No es un motor: no hay editor de escenas ni física, y Godot gana aquí. |
| UI herramienta (15 %) | **9** | Es HTML/CSS con React o Svelte: listas virtualizadas, forms, accesibilidad, texto perfecto. La terminal embebida futura sería xterm.js (el mismo que usa VS Code) + `portable-pty` en Rust. |
| Multiplataforma y empaquetado (15 %) | **7** | El bundler genera dmg/app, MSI/NSIS y AppImage/deb/rpm. El updater exige firma y acepta un JSON estático en GitHub Releases ([updater](https://v2.tauri.app/plugin/updater/)). Se resta por tener tres motores web distintos (ver debilidades). |
| Recursos always-on (10 %) | **8** | No empaqueta Chromium y el backend Rust en reposo consume casi nada. Siendo honesto: en una app real, Better Stack midió 109 MB contra 128 MB de Electron en idle, una diferencia modesta. La diferencia de instalador sí es grande: 57 MB contra 323 MB ([Better Stack](https://betterstack.com/community/guides/scaling-nodejs/tauri-vs-electron-vs-deno-vs-electrobun/)). Las cifras de "5x menos RAM" que circulan no son reproducibles y no las uso. |
| Velocidad dev + LLMs (15 %) | **7** | TypeScript y PixiJS están entre lo que mejor conocen los LLMs. Rust añade fricción, pero el backend de este producto es pequeño (≈5 comandos y un servidor HTTP), y los LLMs escriben bien el Rust de axum, sysinfo y serde. |
| Madurez y riesgo (10 %) | **8** | Lleva dos años estable en v2, tiene gobernanza sin dueño corporativo único y un socio comercial (CrabNebula). El riesgo es la migración a la v3. |

**Ponderado ≈ 7,9 / 10.**

## Las 3 debilidades más serias (y su mitigación)
1. **Tres WebViews distintos (WKWebView, WebView2, WebKitGTK), y Linux es el punto débil.** La propia documentación de Tauri reconoce ventanas en blanco y crashes con NVIDIA y el renderer DMABUF. También reconoce que WebGL puede caer en un camino lento sin dar ningún error, porque WebKitGTK oculta el renderer real ([Linux graphics](https://v2.tauri.app/develop/debug/linux-graphics/)).
   - *Mitigación:* empezar por macOS y Windows, que son el público de Claude Code, Codex y Gemini CLI. En Linux, aplicar las variables documentadas (`WEBKIT_DISABLE_DMABUF_RENDERER`, etc.) en `main()` y ofrecer el renderer Canvas de Pixi 8.16 como opción. El runtime CEF de la v3 es una salida futura si Linux pasa a ser prioritario.
2. **Dos lenguajes (Rust + TS) para un solo dev.** Más tiempo de compilación y el borrow checker.
   - *Mitigación:* el backend es fino y estable, y toda la lógica de producto (estado del semáforo, reducers de eventos) vive en TS. El sistema de capabilities hace explícito lo que el frontend puede invocar, y conviene: el servidor de hooks recibe input externo, así que el webview no debería poder lanzar procesos arbitrarios.
3. **No es un motor de juego.** Hay que montar uno mismo el tilemap, las animaciones, el pathfinding de los avatares y el render-on-demand.
   - *Mitigación:* la "oficina" es casi estática (salas fijas, unos pocos sprites animados por agente). Basta con Tiled + Pixi, y se detiene el ticker cuando no hay cambios para no gastar CPU todo el día.

Riesgo extra: la v3 obligará a migrar en algún momento. La v2 seguirá recibiendo parches mientras tanto (la 2.12.5 salió esta misma semana).

## Ataques previstos y respuesta

**Electron: "El mismo Chromium en todas partes y Node trae `child_process` y `node-pty` sin Rust. La ventaja en RAM es un mito."**
Concedo que la diferencia de RAM en idle es pequeña y que la consistencia de render es mejor en Electron. Pero:
- El instalador es 5-6 veces más pesado, y el updater/firma de Tauri viene integrado.
- El backend de sistema en Rust (tokio, sysinfo, notify) no compite con el event loop de Node ni con GC pauses en una app que corre 24/7.
- Lo decisivo es que **el lock-in es mínimo**. La capa visual (Pixi + UI web) es la misma en ambos. Si Linux/WebKitGTK resultara inviable, se migra a Electron, o a la v3 con CEF, reescribiendo solo unos cientos de líneas de backend. Electron no ofrece la opción inversa con la misma facilidad.

**Godot 4: "Es un motor de verdad: TileMap, Camera2D con zoom, AnimatedSprite y editor visual, sin pegamento."**
Cierto para el 15 % de juego. Pero el producto es en su mayor parte una herramienta:
- Paneles de logs, texto con scroll largo, forms y una terminal embebida, que en Godot exigen construir controles a mano y no tienen un equivalente maduro de xterm.js.
- Godot no trae servidor HTTP (solo `TCPServer`, sobre el que habría que implementar HTTP a mano). Listar procesos por nombre con su cwd obliga a llamar a `ps`/`wmic` o escribir una GDExtension, es decir, volver a C++ o Rust.
- Los LLMs mezclan todavía las APIs de Godot 3 y 4 en GDScript.

Tauri gana claramente en los dos criterios que juntos pesan el 35 % (sistema + UI) y cede poco en el de juego.

*(Flutter+Flame y la extensión de VS Code son rivales más débiles aquí. Flame tiene un ecosistema 2D menor y la integración de sistema exige FFI o plugins. La extensión no es always-on: muere al cerrar VS Code y queda atada al ciclo de vida del editor.)*

## Conclusión
Tauri v2 + Pixi es la combinación con mejor suma ponderada para "Agent Office": un backend nativo, pequeño y seguro para procesos y hooks, la mejor UI de herramienta posible (la web), un renderer 2D suficiente y probado, distribución firmada con updater integrado, y una vía de escape barata si un WebView concreto falla.
