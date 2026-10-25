# Ronda 1: Electron + TypeScript + PixiJS (o Phaser)

## Tesis
Agent Office es sobre todo una **herramienta de sistema que monitoriza CLIs de Node** (Claude Code, Codex y Gemini CLI son paquetes npm), y por encima lleva una capa visual 2D modesta. Electron pone Node completo en el proceso principal y Chromium en el renderer. Es el mismo stack que ya usan las herramientas del sector: VS Code, Cursor, la app de escritorio de Claude y la app Codex de OpenAI, que es Electron "para poder llegar después a otros SO" ([Simon Willison](https://simonwillison.net/2026/Feb/2/introducing-the-codex-app/), [DevClass](https://www.devclass.com/development/2026/02/05/openai-codex-app-looks-beyond-the-ide-devs-ask-why-mac-only/4090132)).

## Estado actual (octubre 2026)
- **Electron 45.0.0** salió el 20-oct-2026 con Chromium 156 y Node 24.22. Las ramas 44 y 43 siguen con parches ([releases.electronjs.org](https://releases.electronjs.org/)).
- **electron-builder 26.18.0** es la versión estable ([GitHub](https://github.com/electron-userland/electron-builder/releases)). Forge integra `@electron/notarize`/`osx-sign`, y en Windows se firma con Azure Artifact Signing o EV mediante `@electron/windows-sign` ([docs](https://www.electronjs.org/docs/latest/tutorial/code-signing)).
- **Auto-update**: `autoUpdater` (Squirrel), `update-electron-app` y el servicio gratuito update.electronjs.org para repos públicos ([docs](https://www.electronjs.org/docs/latest/tutorial/updates)).
- **2D**: PixiJS v8.17 ([blog](https://pixijs.com/blog/tags/updates)) + pixi-viewport 6.x para v8 ([npm](https://npmjs.com/package/pixi-viewport)); o Phaser 4 estable desde abril de 2026, con tilemaps de Tiled y cámara incluidos ([phaser.io](https://phaser.io/download/stable)).
- **Terminal**: node-pty (ConPTY en Windows 10 1809+) + xterm.js 6. Es literalmente el par que usa la terminal de VS Code ([node-pty](https://github.com/microsoft/node-pty), [xterm 6.0](https://newreleases.io/project/npm/@xterm/xterm/release/6.0.0)).

## Puntuación por criterio

| Criterio (peso) | Nota | Justificación |
|---|---|---|
| Integración sistema (20%) | **9** | `http.createServer` en 127.0.0.1 para los hooks. `fs.watch` recursivo (Linux desde Node 19.1, [release](https://nodejs.org/en/blog/release/v19.1.0)) o chokidar para tail de JSONL. `process.kill(pid,'SIGTERM')`, que en Windows termina el proceso sin condiciones ([Node docs](https://nodejs.org/api/process.html#signal-events)). `utilityProcess` aísla los watchers del main ([docs](https://www.electronjs.org/docs/latest/api/utility-process)). Los hooks de Claude Code ya mandan `cwd`/`session_id` en el JSON, y eso se parsea de forma nativa. Le quito 1 punto porque descubrir procesos ajenos (cwd en Windows) exige `ps`/`lsof`/PowerShell, igual que en cualquier otro stack. |
| 2D juego (15%) | **8** | Pixi o Phaser cubren de sobra sprites, tilemap, zoom/pan y animaciones para 30 rooms. No hay editor visual de escenas como en Godot. |
| UI herramienta (15%) | **10** | HTML/CSS + React/Svelte para paneles, listas, formularios y texto con selección, accesibilidad e i18n. Terminal embebida con node-pty + xterm.js sin puentes: el pty vive en el mismo runtime que el resto de la lógica. |
| Multiplataforma/distribución (15%) | **9** | El mismo Chromium en los tres SO, así que la app se renderiza igual en todos. dmg/nsis/AppImage/deb/rpm, notarización y auto-update ya documentados. Le resto 1 por el coste de los certificados (Apple, Windows), que es igual para todos los candidatos. |
| Recursos always-on (10%) | **5** | Es la debilidad real (ver abajo). |
| Velocidad dev + LLMs (15%) | **10** | TS + Node + Electron es probablemente el stack de escritorio con más corpus de entrenamiento. Un solo lenguaje de punta a punta y npm para todo (ps-list, chokidar, pidusage). |
| Madurez/riesgo (10%) | **9** | Más de 10 años, proyecto de la OpenJS Foundation, ciclo de 8 semanas y lo usan las apps de IA más relevantes de 2026. |

**Ponderado: 8.75 / 10**

## Mis 3 debilidades más serias (y cómo se mitigan)
1. **RAM y tamaño.** Cada app trae su Chromium: instaladores de 80-150 MB y 100-300 MB en reposo según la app ([OpenReplay](https://blog.openreplay.com/comparing-electron-tauri-desktop-applications/)). *Mitigación*: una sola ventana y `backgroundThrottling`. El ticker de Pixi se detiene con `visibilitychange` o al minimizar, y el render pasa a ser por eventos (las rooms solo se animan cuando cambia su estado). La lógica pesada va en un único `utilityProcess`. Con 30 agentes la carga útil son unos KB por segundo de eventos HTTP, no gráficos pesados.
2. **Ciclo de actualizaciones y módulos nativos.** Solo se mantienen las 3 últimas majors, y node-pty debe recompilarse contra la ABI de cada Electron. *Mitigación*: `@electron/rebuild` integrado en Forge/builder, binarios precompilados en CI matrix, y node-pty únicamente cuando llegue la terminal (el MVP no usa nativos). Renovate + subir de major cada trimestre.
3. **Superficie de seguridad.** Node junto a Chromium, más un servidor HTTP local. *Mitigación*: `contextIsolation` + `sandbox` en el renderer, un preload mínimo con IPC tipado, cero contenido remoto y CSP estricta. El servidor de hooks valida `Host`/`Origin` contra DNS rebinding y usa un token por instalación inyectado en la config de los hooks. Este riesgo del servidor local lo tienen los cinco candidatos.

## Ataques previsibles

**Tauri: "Electron gasta 5 veces más RAM y pesa 10 veces más".** El tamaño es cierto: Tauri gana en instalador. La cifra de RAM está inflada. Better Stack midió en macOS **109 MB en Tauri contra 128 MB en Electron** en reposo, contando en Tauri los procesos auxiliares del WebView del sistema ([Better Stack](https://betterstack.com/community/guides/scaling-nodejs/tauri-vs-electron-vs-deno-vs-electrobun/)). La propia prensa técnica señala que el "96 % más pequeño y 5x menos RAM" se repite sin benchmark reproducible ([Tech Insider](https://tech-insider.org/tauri-vs-electron-2026/)). Además, para este producto Tauri obliga a escribir procesos, pty y watchers en Rust, o a meter un sidecar de Node, que se come la ventaja de RAM. También hereda WebViews distintos en cada SO (WebKit en macOS/Linux, WebView2 en Windows), con diferencias de WebGL y de CSS que un solo dev tiene que depurar tres veces. Y la capa visual (TS + Pixi) es idéntica en ambos: si algún día la RAM fuera decisiva, el renderer se porta a Tauri sin reescribirlo. Electron es la opción de menor riesgo que no cierra esa puerta.

**Extensión de VS Code: "ya es Electron y el usuario ya tiene VS Code abierto, así que la RAM es cero".** Eso ata el producto a un editor, cuando el objetivo es *centralizar* agentes que corren en terminales sueltas, iTerm, Windows Terminal o tmux. El servidor de hooks tendría que vivir en el extension host, que muere o se duplica con cada ventana de VS Code: N ventanas significa N servidores peleando por el puerto. El webview queda limitado a un panel, con API de mensajes restringida, sin ventana propia ni tray y sin funcionar con VS Code cerrado. Además deja fuera a quien use Cursor/JetBrains/Zed o a quien no use editor. Electron da los mismos componentes (Node + Chromium + Pixi + xterm) **sin** esas restricciones.

## Conclusión
Para un solo dev que programa con LLMs, que necesita Node nativo para hablar con CLIs de Node, una UI de herramienta rica y una terminal embebida a futuro, Electron maximiza 5 de los 7 criterios. Paga un coste conocido y acotable en RAM, en el criterio de menor peso (10 %).
