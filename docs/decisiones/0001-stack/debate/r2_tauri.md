# R2 – Réplica: Tauri v2 + TS + PixiJS

## 1. Afirmaciones débiles de los rivales

**Electron (8,75)**
- **"Integración 9" con un hueco que ellos mismos admiten.** Electron reconoce que necesita `ps`, `lsof` o PowerShell para obtener el cwd de procesos ajenos. Es justo el dato que identifica cada oficina, y Tauri lo obtiene en proceso con `sysinfo::Process::cwd()`, junto con `start_time()` y `kill_with(Signal)` ([docs.rs](https://docs.rs/sysinfo/latest/sysinfo/struct.Process.html)). La afirmación de que hay que hacerlo "igual en cualquier stack" es falsa.
- **"Velocidad 10".** Electron también trabaja con dos contextos (main/`utilityProcess` y renderer), un IPC tipado y un preload, además de recompilar node-pty contra la ABI de cada major. Lo dicen en su debilidad 2. Eso merece un 9, no un 10.
- **"Tauri obliga a usar un sidecar de Node".** Es falso. sysinfo, notify, axum y portable-pty cubren el MVP y la terminal futura sin Node.

**Godot (7,4)**
- **Su plan de sistema en C# tiene el mismo hueco.** `System.Diagnostics.Process` no expone el directorio de trabajo de procesos ajenos: `StartInfo.WorkingDirectory` solo tiene valor si el proceso lo lanzaste tú ([hilo](https://vbdotnetforums.com/threads/shortcut-start-in.12401)). Haría falta P/Invoke o `lsof`. El 7 que se ponen en integración depende además de añadir un segundo runtime (.NET) dentro del motor. Es el mismo problema de "dos lenguajes" que me reprochan a mí, pero más pesado.
- **"2D = 10".** El 10 sería válido para un juego. Aquí las 30 oficinas son casi estáticas, y un editor de tilesets con autotiling aporta poco frente a Tiled + Pixi. Le daría un 9.

**Flutter (7,1)**
- **"Un solo lenguaje y sin puente".** Ellos mismos reconocen que listar procesos requiere FFI a `libproc` o `/proc`, o PowerShell. FFI es un puente, y más frágil que un comando Tauri con serde.
- **La terminal futura.** `xterm` + `flutter_pty` llevan unos 2 años sin publicar versión, según su propio texto. xterm.js 6 es el estándar de la industria, y Tauri lo usa directamente.

**Extensión de VS Code (8,2)**
- **"Tauri tiene que mantener su propio updater".** Es falso. `tauri-plugin-updater` es oficial y acepta un JSON estático en GitHub Releases ([updater](https://v2.tauri.app/plugin/updater/)). Linux, además, no exige firma.
- **El mejor argumento de la extensión se vuelve en su contra.** Pixel Agents, su prueba de concepto, tuvo que añadir un modo independiente con `npx pixel-agents` para cubrir tmux y terminales que no son VS Code. Y por ahora solo soporta Claude Code ([repo](https://github.com/pablodelucca/pixel-agents)). La referencia del dominio confirma que el producto necesita vivir fuera del editor.
- **"UI 9" con terminales de VS Code.** `Terminal.processId` solo sirve para agentes lanzados *desde* VS Code. Los que corren en iTerm o Windows Terminal siguen necesitando la misma detección por procesos que cualquier otro stack.

## 2. Respuesta a los ataques recibidos

- **"Dos lenguajes y un bridge" (Flutter, Electron, VS Code).** El backend en Rust es pequeño: `list_agents`, `kill_agent`, un servidor de hooks y un tailer de JSONL, todos con crates estándar que los LLMs escriben bien. Toda la lógica de producto (máquina de estados del semáforo, UI) va en TS. Electron tiene el mismo patrón main↔renderer, solo que con un único lenguaje.
- **"WebViews distintos, depurar tres veces" (Electron, VS Code).** Es el ataque legítimo y no lo niego. Lo acoto así:
  - Pixi abstrae el render, y la UI de herramienta es CSS estándar.
  - Windows usa WebView2 (Chromium), así que la diferencia real es con WebKit.
  - El problema grave está en Linux/WebKitGTK, documentado con workarounds y con una ruta de fallback a Canvas ([Linux graphics](https://v2.tauri.app/develop/debug/linux-graphics/), [Pixi 8.16](https://pixijs.com/blog/8.16.0)).
- **"La RAM es un mito".** Ya lo concedí en R1: 109 contra 128 MB en idle ([Better Stack](https://betterstack.com/community/guides/scaling-nodejs/tauri-vs-electron-vs-deno-vs-electrobun/)). Mi argumento nunca fue la RAM. Es el backend nativo, el instalador unas 5-6 veces menor y el modelo de capabilities frente a un servidor HTTP que recibe input externo.

## 3. Concesiones honestas y ajustes

- **Electron me gana en consistencia multiplataforma.** Tener un solo Chromium es objetivamente menos riesgo. Bajo mi "Multiplataforma" de 7 a **6,5** si Linux está en el MVP. Si es solo macOS y Windows, lo mantengo en 7.
- **Electron también me gana en velocidad para un solo dev.** Le doy 9 frente a mi 7, por usar un solo lenguaje y un corpus más grande.
- **Godot me gana en 2D**, por 9 contra mi 7.
- **La extensión de VS Code gana en distribución inicial**: sin firma ni notarización, y llega a usuarios desde el día 1. Concedo que es una buena vía de *validación*, pero no de producto.
- **Mi ponderado queda en ≈7,8.** Mi recálculo honesto del de Electron, con velocidad 9 e integración 8 por el hueco del cwd, es ≈8,4.

La diferencia real es Electron contra Tauri, y se decide por el criterio con más peso: la integración de sistema, 20 %. Sobre ese criterio ningún rival ha dicho nada que yo deba conceder.
