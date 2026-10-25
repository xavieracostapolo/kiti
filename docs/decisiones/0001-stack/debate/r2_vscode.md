# R2 — Réplica: Extensión de VS Code

## Corrección propia
En R1 dije que la shell integration API era "estable desde la 1.93". Las release notes solo dicen *"The terminal shell integration API is now available to use"* ([v1_93](https://code.visualstudio.com/updates/v1_93)). Eso significa que pasó de proposed a API pública, pero no hay una garantía de "estable" adicional. Retiro la palabra y la sustituyo por **"pública desde 1.93"**.

## Ataques a los rivales

**Tauri**
1. Pone **9 en UI**, pero la terminal futura (xterm.js + `portable-pty`) necesita un puente de streaming Rust↔JS que no tiene en cuenta. Su "backend de ≈5 comandos" ignora tres formatos de transcript, la detección de procesos y, más adelante, el PTY.
2. Se pone **7 en multiplataforma**, pero recomienda "empezar por macOS y Windows". Además, su propia fuente admite que WebGL en WebKitGTK puede caer sin avisar a un camino lento. Eso también le quita puntos en el criterio de 2D.
3. Pone **8 en recursos frente al 5 de Electron**, con su propia cifra de 109 MB contra 128 MB. Una diferencia de unos 15 % no justifica 3 puntos. Y `sysinfo::kill_with` solo garantiza `Kill` en todas las plataformas ([docs.rs](https://docs.rs/sysinfo/latest/sysinfo/struct.Process.html)).

**Electron**
1. Se pone **10 en UI y 10 en velocidad de desarrollo**, pero admite que node-pty hay que recompilarlo contra cada ABI de Electron. Con ese costo reconocido, ninguno de los dos llega a 10.
2. Me acusa de "dejar fuera a Cursor", y es falso: Open VSX cubre Cursor, Windsurf y VSCodium ([Cursor docs](https://cursor.com/docs/configuration/extensions)). Para JetBrains y Zed sí tiene razón.
3. Dice que el producto queda limitado a "agentes en VS Code". También es falso. El `core` detecta procesos de todo el sistema con `ps`/`Get-CimInstance`. Los hooks de Claude Code se configuran a nivel de usuario (`~/.claude/settings.json`), así que un `claude` lanzado en iTerm o en tmux también llega al servidor. La extensión ve todo, aunque el panel viva en el editor.

**Godot**
1. Pone **UI 7** apoyándose en GodotXterm para la terminal. La última versión que encontré, 4.0.3, está compilada para Godot 4.3. Las versiones anteriores tenían el PTY "solo Linux y macOS", y no encontré confirmación de PTY en Windows en la rama 4.x ([asset library](https://godotengine.org/asset-library/asset/1007), [v2.0.0](https://github.com/lihop/godot-xterm/releases/tag/v2.0.0)).
2. Su 7 en sistema solo vale con C#. Eso supone dos lenguajes (GDScript + C#), el runtime .NET y LLMs que se confunden entre Godot 3 y 4. Velocidad 6 es generoso.

**Flutter + Flame**
1. En **recursos 7** reconoce que Flame dibuja cada frame aunque no pase nada. En una app que queda abierta todo el día, eso es el criterio entero, no un detalle que se arregla con diseño.
2. Pone **UI 8** contando con `xterm` y `flutter_pty`, que él mismo dice que llevan unos 2 años sin publicar versión.
3. Dice que yo "dependo de la API de webviews de Microsoft". Es una API pública desde 2017 con compatibilidad hacia atrás, y la usan Pixel Agents y miles de extensiones. Es menos riesgo que un desktop que acaba de cambiar de mantenedor.

## Respuesta a los ataques recibidos
- **"Muere al cerrar VS Code" (Tauri, Godot, Flutter).** Es cierto y lo concedo. El brief dice "app de escritorio" y "always-on", y mi candidato no cumple eso por sí solo.
- **"N ventanas, N servidores peleando por el puerto" (Electron).** Se resuelve con un lockfile y elección de líder: solo una ventana sirve y las demás se conectan como clientes. Pixel Agents ya funciona con "servidores activos" en varias ventanas ([repo](https://github.com/pablodelucca/pixel-agents)). Electron tiene un problema equivalente: necesita `requestSingleInstanceLock`.
- **"API de mensajes restringida".** `postMessage` con JSON es equivalente a un IPC tipado de Electron. Para esta app no es una limitación real.

## Concesiones honestas
1. **Electron me gana como app final.** Tiene el mismo stack sin depender del editor: ventana propia, tray, always-on-top, siempre activo.
2. **Bajo dos notas.** Sistema pasa de 8 a **7**: el host compartido, la elección de líder y depender del editor abierto pesan más de lo que admití. Multiplataforma y distribución pasa de 9 a **7**: distribuir es trivial, pero entregar una "app de escritorio" no lo es. Mi ponderado queda en **≈7,7**.
3. El dato de Flutter sobre Canonical es correcto ([OMG! Ubuntu](https://www.omgubuntu.co.uk/2026/05/flutter-desktop-canonical-maintained)).

## Posición revisada
Propongo una vía híbrida, no exclusiva:
- **`core` en Node y `ui` en PixiJS**, separados del contenedor.
- **Primero la extensión**, para llegar en días a usuarios reales con instalación de un clic, terminal real y actualizaciones gratis.
- **Después un shell de Electron**, que reutiliza los mismos paquetes, cuando la validación justifique la ventana propia.

En ese plan, VS Code es el canal de entrada de menor costo y Electron el destino natural. Es la misma estrategia que Pixel Agents siguió con su modo `npx`.
