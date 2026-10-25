# Veredicto del juez: framework para "Agent Office"

## 1. Auditoría de hechos

| # | Afirmación | Estado | Fuente |
|---|---|---|---|
| 1 | RAM en reposo: Tauri 109 MB y Electron 128 MB; bundle de 57 MB contra 323 MB | **Confirmada, con matiz**: es una sola app (un grabador de pantalla) medida en macOS. El "5x menos RAM" que circula es **exagerado** | [Better Stack](https://betterstack.com/community/guides/scaling-nodejs/tauri-vs-electron-vs-deno-vs-electrobun/) |
| 2 | `sysinfo` obtiene en proceso el `cwd` de procesos ajenos, y "en cualquier stack hace falta `ps`/`lsof`" | **Confirmada para Tauri**: lo saca con `proc_pidinfo` en macOS y leyendo el PEB con `ReadProcessMemory` en Windows. Si falla, devuelve `None` sin avisar. Lo de "igual en cualquier stack" es **exagerado**: en Windows, desde Node, Dart o C# no hay forma de obtenerlo sin código nativo | [docs.rs](https://docs.rs/sysinfo/latest/sysinfo/struct.Process.html), [process.rs (Windows)](https://github.com/GuillaumeGomez/sysinfo/blob/main/src/windows/process.rs) |
| 3 | En Windows, `kill_with` solo funciona con `Kill` | **Confirmada**: internamente llama a `taskkill /F`. Ningún stack ofrece un SIGTERM real en Windows | [process.rs](https://github.com/GuillaumeGomez/sysinfo/blob/main/src/windows/process.rs) |
| 4 | En WebKitGTK, WebGL cae sin error a un camino lento y el renderer real queda oculto | **Confirmada**. La mitigación de Tauri ("fallback Canvas de Pixi 8.16") es **exagerada**: ese renderer es *experimental* y solo se activa solo si **no hay** WebGL. Como en WebKitGTK el contexto WebGL sí se crea, el fallback no se dispara y hay que forzarlo a mano | [Tauri Linux](https://v2.tauri.app/develop/debug/linux-graphics/), [Pixi 8.16](https://pixijs.com/blog/8.16.0) |
| 5 | `OS.kill` de Godot envía SIGKILL y `is_process_running` solo sirve para procesos hijos | **Confirmada** | [os_unix.cpp](https://github.com/godotengine/godot/blob/master/drivers/unix/os_unix.cpp) |
| 6 | `flutter_pty` 0.4.2 lleva unos 21 meses sin publicar; `xterm` 4.0.0, unos 2 años | **Confirmada** | [flutter_pty](https://pub.dev/packages/flutter_pty), [xterm](https://pub.dev/packages/xterm) |
| 7 | GodotXterm es una terminal madura y multiplataforma | **Exagerada**: el repo dice "primarily developed and tested on Linux" y no documenta que el PTY funcione en Windows | [repo](https://github.com/lihop/godot-xterm) |
| 8 | Pixel Agents valida el stack VS Code + Pixi | **Exagerada**. Lo que sí es cierto: 9,9k estrellas, solo soporta Claude Code, usa hooks + JSONL y tiene un modo `npx` independiente. Pero dibuja con **Canvas 2D + React, no con Pixi** | [repo](https://github.com/pablodelucca/pixel-agents) |
| 9 | "Los tres CLIs son paquetes de Node" (Electron) | **Falsa para 2 de 3**. Claude Code es un binario nativo que no invoca Node. Codex se reescribió en Rust. Gemini CLI sí corre sobre Node 20+, así que su proceso aparece como `node` y no como `gemini` | [Claude install](https://code.claude.com/docs/en/getting-started/installation), [InfoQ](https://www.infoq.com/news/2025/06/codex-cli-rust-native-rewrite), [Gemini](https://www.mintlify.com/google-gemini/gemini-cli/installation) |
| 10 | Los tres CLIs ofrecen hooks utilizables | **Confirmada, con asimetría**. Claude Code tiene un tipo `http` nativo y manda `cwd`, `session_id` y `transcript_path`. Codex tiene hooks activos por defecto, pero solo de tipo `command`/`mcp_tool`. Gemini solo admite `command` | [Claude hooks](https://code.claude.com/docs/en/hooks), [Codex hooks](https://learn.chatgpt.com/docs/hooks), [Gemini hooks](https://geminicli.com/docs/hooks/) |
| 11 | Tauri v3 tiene un runtime CEF; Canonical pasa a ser *steward* de Flutter desktop | **Confirmada**. Ojo: Tauri v3 está en alpha | [v3 alpha](https://newreleases.io/project/github/tauri-apps/tauri/release/tauri-v3.0.0-alpha.0), [OMG! Ubuntu](https://www.omgubuntu.co.uk/2026/05/flutter-desktop-canonical-maintained) |

No verifiqué que "la app Codex es Electron": no cambia el resultado.

## 2. Puntuación independiente

| Criterio (peso) | Tauri | Electron | Godot (C#) | Flutter | VS Code |
|---|---|---|---|---|---|
| Sistema (20%) | 9 | 8 | 7 | 7 | 7 |
| 2D juego (15%) | 7 | 8 | 9 | 7 | 7 |
| UI herramienta (15%) | 8 | 9 | 6 | 7 | 8 |
| Multiplataforma/firma (15%) | 7 | 9 | 7 | 6 | 5 |
| Recursos (10%) | 8 | 5 | 6 | 7 | 5 |
| Velocidad dev + LLMs (15%) | 7 | 9 | 5 | 6 | 8 |
| Madurez/riesgo (10%) | 8 | 9 | 8 | 6 | 7 |
| **Ponderado** | **7,75** | **8,25** | **6,85** | **6,60** | **6,80** |

Por qué estas notas:
- **Electron, 2D 8.** Es el mismo Pixi que en Tauri, pero sobre el mismo Chromium con WebGL/WebGPU en los tres SO. Esa diferencia sí es técnica.
- **Tauri, sistema 9.** Es la única ventaja de integración real que vi en todo el debate, por el punto 2 de la auditoría.
- **VS Code, multiplataforma 5.** No entrega una "app de escritorio always-on".
- **Godot, velocidad 5.** Son dos lenguajes, más el runtime .NET, más LLMs que confunden Godot 3 con 4.

## 3. Lo que el debate pasó por alto

1. **El núcleo de detección es el mismo en cualquier stack, y los hooks ya traen el `cwd`.** Los tres CLIs envían `cwd` y el id de sesión. El escaneo de procesos solo sirve para saber si el agente sigue vivo y para mapear su PID. Eso le quita importancia a la ventaja de `sysinfo`. Además, la detección tiene que buscar en la **línea de comando**, no en el nombre del proceso, porque Gemini aparece como `node`.
2. **La latencia del forwarder de hooks.** Codex y Gemini solo ejecutan hooks de tipo `command`, de forma síncrona y en cada uso de herramienta. Si ese forwarder es el propio binario de Electron (`ELECTRON_RUN_AS_NODE`), cada llamada añade un arranque lento dentro del bucle del agente. Hace falta `curl` o un binario mínimo, con timeout corto y que falle en abierto: si la app está cerrada, el agente no debe bloquearse.
3. **El 2D es trivial.** Pixel Agents lo resuelve con Canvas 2D. Unas 30 salas casi estáticas con unos pocos sprites no justifican un motor. La ventaja de Godot no pesa aquí.
4. **Lo que de verdad queda encendido todo el día es el daemon, no la ventana.** Si se destruye el renderer al cerrar a la bandeja, la penalización de RAM de Electron baja mucho. Hay que medirlo.
5. **Tauri v3 con CEF** elimina el riesgo de WebKitGTK, pero también su ventaja de tamaño. Si madura, Tauri y Electron convergen.
6. **El coste de migrar después depende de la arquitectura, no del framework** (ver sección 5).

## 4. Veredicto

- **Ganador: Electron + TypeScript + PixiJS.** Un solo lenguaje con el mayor corpus para LLMs, el mismo render en los tres SO (Linux incluido, donde WebKitGTK es un riesgo real para un canvas WebGL) y el par node-pty + xterm.js ya probado para la terminal futura. Su debilidad, la RAM, cae en el criterio de menor peso (10%), y la diferencia medida es pequeña.
- **Segundo: Tauri v2.** Tiene mejor backend de sistema y mejor huella.
- **Cuándo cambiaría el ganador:**
  - Tauri pasa a ganar si el consumo en reposo se convierte en un requisito duro y medido (por ejemplo, menos de 100 MB con la ventana abierta), si Linux queda fuera del alcance, o si el dev ya domina Rust.
  - Godot solo gana si el diferencial del producto pasa a ser la experiencia de juego (avatares con pathfinding, iluminación, partículas) por encima de la herramienta.

## 5. Stack exacto y arquitectura anti lock-in

**Monorepo pnpm con contrato por protocolo, no por framework:**

- **`packages/protocol`**: tipos y esquemas `zod` de `AgentEvent` y `AgentSnapshot`, versionados.
- **`packages/core`** (Node puro, sin imports de Electron):
  - `HookServer` con `node:http` en 127.0.0.1, token por instalación y validación de `Host`.
  - `ProcessScanner`: interfaz con implementación `ps`/`/proc`/PowerShell; un addon napi-rs sobre `sysinfo` queda como opción para el `cwd` en Windows.
  - `TranscriptTailer` con `chokidar` v4.
  - Reducer del semáforo.
  - `killAgent`: SIGTERM y luego SIGKILL; en Windows, `taskkill /T`.
  - Expone un WebSocket con `ws`.
- **`packages/ui`**: Vite + React 19 + PixiJS v8 + `pixi-viewport` v6. Mapas de Tiled en JSON. Ticker en modo render-on-demand. Habla con el core solo a través de una interfaz `Transport`.
- **`apps/desktop`**: Electron (última estable) con `electron-vite`. El core corre en `utilityProcess`. Renderer con `contextIsolation` + `sandbox`. Distribución con `electron-builder` + `electron-updater` sobre GitHub Releases, notarización Apple y Azure Artifact Signing.
- **`apps/hook-forwarder`**: un binario Go o Rust mínimo (o `curl`) para los hooks de Codex y Gemini. Claude Code usa `type: "http"` directamente.
- **`apps/cli`** (opcional): `npx agent-office` sirve la misma UI en el navegador. Es la prueba continua de que no hay lock-in, y deja abierta una futura extensión de VS Code.
- **Más adelante**: `node-pty` + `@xterm/xterm`.

**Si algún día se migra a Tauri:** el core se empaqueta como sidecar (`bun build --compile` o Node SEA) o se reescribe en Rust, unas 1-2k líneas contra un protocolo ya fijado. `packages/ui` no cambia.
