# Ronda 2 — Réplica de Godot 4

## 1. Afirmaciones débiles de los rivales

**Electron (8,75): es el puntaje más inflado.**
- Pone un **10 en UI** y un **10 en LLMs**, pero con un 5 en recursos, que es el criterio clave para una app que queda abierta todo el día. Ellos mismos citan instaladores de 80-150 MB y 100-300 MB de RAM en reposo, y aun así tratan ese coste como "acotable".
- Su 2D recibe un **8**, mientras que el de Tauri, con exactamente el mismo PixiJS, recibe un 7. La librería es la misma; el punto de diferencia es preferencia, no un dato técnico.
- Cuenta `ps-list` como ventaja de npm. Ese paquete no devuelve `cwd`, así que el descubrimiento de procesos sigue requiriendo `lsof` o PowerShell, igual que en cualquier otro stack. El propio Electron lo admite.

**Tauri (7,9).**
- Pone un **8 en recursos** frente al 5 de Electron, pero su propia fuente mide 109 MB contra 128 MB en reposo ([Better Stack](https://betterstack.com/community/guides/scaling-nodejs/tauri-vs-electron-vs-deno-vs-electrobun/)). Un 15 % de diferencia no justifica 3 puntos.
- Da un **9 en UI** cuando reconoce que WebKitGTK puede degradar WebGL sin avisar ([docs Tauri](https://v2.tauri.app/develop/debug/linux-graphics/)). Eso afecta justo al mapa, que es el corazón del producto, en uno de los 3 SO del brief.
- Tiene dos lenguajes y un puente IPC para un solo dev. Su respuesta es que "el backend es fino". Eso es igual de cierto para mi capa C#, que además corre en el mismo proceso que la escena.

**Flutter (7,1).**
- El traspaso del desktop a Canonical es real ([OMG! Ubuntu](https://www.omgubuntu.co.uk/2026/05/flutter-desktop-canonical-maintained)), pero eso significa que Google *delegó* el desktop. Además ocurrió hace 5 meses y no tiene historial que lo respalde. Eso no justifica "riesgo bajo".
- `xterm` y `flutter_pty` llevan unos 2 años sin versión, y Flame dibuja cada frame aunque no pase nada. Ellos mismos lo admiten, y lo primero choca de frente con "terminal futura" y lo segundo con "always-on".

**VS Code (8,2).**
- Le da un **9 en "Multiplataforma y distribución"** a algo que no es una app de escritorio. El brief dice "escritorio primero" y la propia extensión admite que no tiene ventana propia, ni bandeja del sistema, ni funciona con el editor cerrado.
- Lo de "la terminal ya existe" con `createTerminal` solo cubre agentes lanzados *desde* VS Code. Los que corren en iTerm, tmux o Windows Terminal, que son justamente los que el producto quiere centralizar, no aparecen ahí.
- Pixel Agents (9,9k estrellas) es real, pero solo soporta Claude Code; Codex y Gemini están en el roadmap ([repo](https://github.com/pablodelucca/pixel-agents)). Además dibuja en Canvas 2D con React, no en PixiJS. Valida el *concepto*, no el stack que propone la extensión.

## 2. Respuesta a los ataques contra Godot

- **"No tiene servidor HTTP, no lista procesos, kill solo con SIGKILL" (Tauri, VS Code).** Ya lo concedí para GDScript. Por eso la propuesta es usar **C#**, que trae `HttpListener`, `FileSystemWatcher` y `Process` en el mismo proceso, sin IPC. Para descubrir el `cwd`, *todos* los candidatos terminan recurriendo a `ps`/`lsof`/PowerShell o a una librería nativa. Tauri lo resuelve con `sysinfo`, que es una crate de Rust, igual que una GDExtension.
- **"No hay un xterm.js maduro" (Tauri).** GodotXterm usa node-pty y libuv para el PTY, tiene licencia MIT y su última publicación es de febrero de 2026 ([Asset Library](https://godotengine.org/asset-library/asset/1007)). Es más reciente que la de `flutter_pty`. No es xterm.js, eso es cierto.
- **"Pelear con un motor para hacer formularios" (Flutter).** El editor de Godot es un formulario gigante: un inspector con miles de campos, un `Tree` y un `CodeEdit`. Desde la 4.5 los `Control` además tienen lectores de pantalla vía AccessKit, en estado experimental ([4.5](https://godotengine.org/releases/4.5/)).
- **"Fundación financiada por donaciones" (Flutter).** Con licencia MIT y una base de código autocontenida, el peor escenario es un fork estable. Si Google pierde interés en el desktop, en cambio, el que se queda sin mantenedor es el embedder.

## 3. Concesiones honestas

1. **La UI web es mejor para texto denso.** Los logs largos, la selección de texto y los formularios complejos salen más rápido en HTML/CSS, y xterm.js es superior a GodotXterm. Mantengo mi 7, pero Tauri y Electron me ganan en ese criterio.
2. **Hay más corpus de LLMs en TypeScript.** Electron y Tauri me ganan en el criterio de velocidad de desarrollo. Mantengo el 6.
3. **Pixel Agents demuestra que una oficina pixel art funciona con Canvas 2D**, lo que reduce la ventaja de Godot en el criterio 2D. Si la oficina es casi estática, el editor de escenas aporta menos de lo que dije. **Bajo mi 2D de 10 a 9**: sigue siendo el mejor, pero la diferencia importa menos.
4. **Recursos: corrijo a la baja.** Con C# el export incluye el runtime de .NET, así que pesa más y probablemente usa más RAM que con GDScript puro. **Bajo de 7 a 6** hasta medirlo.
5. **No hay auto-updater integrado**, algo que Electron y Tauri sí traen.

**Nuevo ponderado: ≈7,2.** Mi posición queda así: Godot es la mejor opción **si el diferencial del producto es la experiencia visual** (avatares que caminan, animaciones de estado, zoom cinematográfico). Si la oficina es un tablero casi estático con mucha UI de herramienta, admito que un stack web (Tauri o Electron) es más pragmático para este dev.
