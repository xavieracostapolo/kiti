# Ronda 2: réplica de Electron

## 1. Afirmaciones débiles de los rivales

**Tauri**
- *"Instalador 57 MB contra 323 MB"*. Esa cifra viene de **una sola app**, un grabador de pantalla con edición de vídeo, y Better Stack no aclara si mide el DMG o el bundle desempaquetado ([Better Stack](https://betterstack.com/community/guides/scaling-nodejs/tauri-vs-electron-vs-deno-vs-electrobun/)). No sirve como regla general. Lo que sí queda en pie es que Tauri ocupa menos en disco. La propia fuente de Tauri da 109 MB contra 128 MB de RAM en reposo, y la RAM es lo que importa en una app always-on.
- *"El lock-in es mínimo; Electron no ofrece la opción inversa con la misma facilidad"*. Es falso, porque la situación es simétrica. El renderer (TS + Pixi) es idéntico en los dos, y el backend de Electron es TS, que incluso podría correr como sidecar de Tauri. Portar Node a Rust cuesta lo mismo que portar Rust a Node.
- *"Backend de unos 5 comandos"*. Se queda corto. El backend tiene que descubrir procesos, levantar el servidor de hooks, hacer tail y parseo de **tres formatos** de transcript (Claude, Codex, Gemini), lanzar agentes y, más adelante, gestionar el pty. Ese parseo cambia con cada versión de los CLIs, y o se hace en Rust o se cruza el bridge. Por último, que Tauri v3 añada **CEF opcional** ([releases](https://github.com/tauri-apps/tauri/releases)) reconoce que la inconsistencia entre WebViews es un problema real.

**Godot**
- *Stack real*: Godot + C#/.NET + GDExtension para la terminal son tres runtimes, y con C# la ventaja de "el motor lo da todo" se diluye. GodotXterm declara que se desarrolla y prueba **"principalmente en Linux"** ([repo](https://github.com/lihop/godot-xterm)), mientras que node-pty es el que usa la terminal de VS Code en Windows a diario.
- Sin auto-updater integrado (ellos mismos lo admiten) y sin cifras de RAM. Su 7 en recursos no está verificado.

**Flutter + Flame**
- *"Un solo proceso, sin IPC"*. El servidor HTTP, el `watch` y el parseo de JSONL en el isolate de UI compiten con el render. Para evitar jank hay que moverlos a isolates, y la comunicación entre isolates por mensajes vuelve a ser IPC.
- *"Gasta claramente menos que Electron"* va sin cifra, después de admitir que Flame redibuja cada frame.
- Terminal: `xterm` y `flutter_pty` llevan unos 2 años sin publicar versión y el multi-ventana no es estable. Es justo la parte "herramienta" que pesa el 15 %.

**Extensión de VS Code**
- Pixel Agents es buena evidencia, pero **a mi favor**. Valida el stack TS + web + Node y, sobre todo, tuvo que crear un modo standalone (`npx pixel-agents` con servidor Fastify y la UI en el navegador) para detectar sesiones lanzadas desde cualquier terminal ([repo](https://github.com/pablodelucca/pixel-agents)). El producto ya se salió del editor. Además, hoy solo soporta Claude Code.
- *"La terminal embebida ya existe"*. Solo sirve para agentes lanzados *desde* VS Code. A un `claude` que corre en iTerm o en tmux no se le puede adjuntar una terminal de VS Code.
- Elegir líder con lockfile para N extension hosts es complejidad distribuida que una app de una sola instancia (`app.requestSingleInstanceLock()`) no tiene.

## 2. Respuesta a los ataques que me hicieron

- **Peso e instalador (Tauri y VS Code)**: lo concedo en disco. En RAM, las mediciones citadas por ambos bandos dan una diferencia de unos 20 MB.
- **"Rust no tiene GC pauses" (Tauri)**: con unos KB/s de eventos HTTP, las pausas de GC de V8 no se notan. Las sufren a diario VS Code y Cursor con cargas mil veces mayores.
- **"Electron obliga a separar main y renderer con IPC" (Flutter)**: sí, y es una ventaja de seguridad. El renderer, en sandbox, nunca toca procesos, y el bridge es `contextBridge` + `ipcRenderer.invoke` tipado, un patrón que los LLMs generan de memoria.
- **"Pixi te obliga a montar tilemap, cámara y animación a mano" (Godot)**: es cierto con Pixi solo. Con **Phaser 4** vienen de fábrica Tilemap (Tiled), cámara con zoom/pan/follow, tweens y animaciones ([phaser.io](https://phaser.io/download/stable)).
- **"Firmar, notarizar y updater cuestan tiempo" (VS Code)**: es un coste real pero se paga una vez. Forge y electron-builder lo dejan en configuración, y Tauri y Flutter lo pagan igual.

## 3. Concesiones y ajustes

- **Godot gana en 2D de juego**: editor de escenas y autotiling. Bajo mi 2D de **8 a 7 con Pixi solo**, y lo mantengo en 8 solo si se elige Phaser 4.
- **Tauri gana en tamaño de distribución**. **VS Code gana en tiempo hasta el primer usuario**: sin firma ni notarización, y con instalación en un clic en Cursor y Windsurf. Es una estrategia de MVP legítima.
- **Subo recursos de 5 a 6**: la evidencia que yo mismo cité muestra un hueco de RAM pequeño frente a Tauri.
- Mantengo **sistema 9, UI 10, LLMs 10**. Ningún rival ha mostrado una terminal embebida más madura que node-pty + xterm.js, ni un lenguaje con más corpus.

**Ponderado revisado (Pixi solo): ~8,7.** La arquitectura sensata para cualquier opción web es `core` (Node) + `ui` (Pixi) + shell. Electron es el shell que menos restricciones impone hoy: sin Rust, sin depender del editor y con el mismo Chromium en todos los SO.
