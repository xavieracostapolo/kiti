# Ronda 1 — Apertura: Godot 4 (recomendación: Godot 4.7 + C#, con GDScript solo para la capa visual)

## Tesis
"Agent Office" es, en el fondo, un **juego 2D con una capa de herramienta encima**. Godot es el único candidato con tilemap, sprites animados, cámara y escena de fábrica, y además una UI de herramienta ya probada en producción: el propio editor de Godot está hecho con los mismos nodos `Control` que usaría esta app. Los otros cuatro candidatos tienen que armar el motor de juego a partir de librerías. Godot, en cambio, tiene que cubrir lo de sistema, y eso se resuelve con C#/.NET.

## Estado actual (verificado)
- La versión estable es la **4.7**. La documentación "stable" ya es la 4.7 y existe la 4.7.1 de mantenimiento ([docs OS](https://docs.godotengine.org/en/stable/classes/class_os.html), [4.7.1](https://gamedev.net/news/4740-godot-engine-471-stable-released/)). Licencia MIT y gobernanza de la Godot Foundation.
- Apps que no son juegos: **Pixelorama** usa Godot 4.7.1, tiene 10,8k estrellas y sale para Win/Linux/macOS ([GitHub](https://github.com/Orama-Interactive/Pixelorama)). A eso se suman el editor de Godot, Material Maker y Lorien.
- `TileMapLayer` (desde la 4.3), `Camera2D` con `zoom` y `position_smoothing`, `AnimatedSprite2D`, `Tween`. Hacer zoom a una oficina es un `Tween` sobre `camera.zoom/position`, unas 10 líneas.
- Procesos: `OS.create_process`, `OS.execute_with_pipe` (IO redirigida), `OS.kill`, `OS.is_process_running` ([docs](https://docs.godotengine.org/en/stable/classes/class_os.html)).
- **Low processor mode**: el motor solo redibuja cuando hace falta y tiene un sleep configurable entre frames (`low_processor_usage_mode_sleep_usec`) ([docs](https://docs.godotengine.org/en/stable/classes/class_os.html)). Es el modo con el que trabaja el editor.
- Firma: para macOS hay codesign/notarytool desde Mac y **rcodesign desde Linux/Windows**, lo que permite notarizar en CI sin Mac ([docs macOS](https://docs.godotengine.org/en/stable/tutorials/export/exporting_for_macos.html)). Windows tiene la firma integrada en el preset de export ([docs Windows](https://docs.godotengine.org/en/stable/tutorials/export/exporting_for_windows.html)).
- Terminal embebida: **GodotXterm** es una GDExtension MIT con nodo `Terminal` (libtsm) y nodo `PTY` (código de node-pty y libuv) para Linux, macOS y Windows ([Asset Library](https://godotengine.org/asset-library/asset/1007)). Hay una alternativa con ConPTY y openpty sobre libvterm ([Godot Store](https://store.godotengine.org/asset/poingstudios/terminal/)).

## Puntuación por criterio

| Criterio (peso) | Nota | Justificación |
|---|---|---|
| Integración con sistema (20%) | **7** | La API nativa de Godot se queda corta: no enumera procesos ajenos, `kill` en Unix manda **SIGKILL** directo y `is_process_running` solo sirve para procesos hijos ([os_unix.cpp](https://github.com/godotengine/godot/blob/master/drivers/unix/os_unix.cpp)). Con **C#**, en cambio, se tiene toda la BCL de .NET: `System.Diagnostics.Process` (PID y StartTime), `FileSystemWatcher` para los JSONL, `HttpListener` o Kestrel para los hooks, y `kill -TERM`/`taskkill` mediante `Process`. Con GDScript puro bajaría a 5. |
| Capacidad 2D de juego (15%) | **10** | Es su terreno: editor de tilesets con autotiling, cámara, animaciones, partículas, shaders, iluminación 2D y pixel snap. Ningún rival trae editor visual de escenas. |
| UI tipo herramienta (15%) | **7** | `Tree`, `ItemList`, `RichTextLabel` (BBCode), `CodeEdit`, `TabContainer`, `SplitContainer`, temas y ventanas nativas múltiples (`Window`). Basta ver el editor. Le faltan el ecosistema de componentes de la web y un CSS, por lo que los formularios complejos llevan más trabajo. |
| Multiplataforma y firma (15%) | **7** | Un solo proyecto exporta a los 3 SO con un clic o por CLI headless, y la notarización corre en CI Linux. Lo que falta es un auto-updater integrado (Tauri y Electron sí lo traen). |
| Recursos always-on (10%) | **7** | Con low processor mode la CPU queda cerca de 0 cuando no hay cambios. Pero un sprite animado obliga a redibujar, así que hay que bajar `Engine.max_fps` (por ejemplo a 10-15 en idle) y pausar animaciones fuera de vista. No traigo cifras de RAM porque no las pude verificar; propongo medirlas en un prototipo. |
| Velocidad dev + LLMs (15%) | **6** | Es mi punto más débil (ver abajo). Hay servidores MCP para Godot ([lista](https://glama.ai/mcp/servers/@LeeSinLiang/godot-mcp/blob/f2423ba8da17e2fc3bf8235c2ea02a6c3bac5762/README.md)) y C# es muy conocido por los LLM. |
| Madurez y riesgo (10%) | **8** | MIT, fundación, ciclo de releases regular y apps de terceros que siguen la última estable. No hay riesgo de que un proveedor cierre el proyecto. |

**Ponderado: unos 7,4/10.**

## Mis 3 debilidades más serias (y cómo se mitigan)
1. **Las API de sistema de GDScript son pobres**: no listan procesos, `kill` solo manda SIGKILL, no hay file watcher ni servidor HTTP. *Mitigación:* usar C# en la capa de servicios (`AgentMonitor`, `HookServer`, `TranscriptTailer`) con .NET estándar, que es código que cualquier LLM escribe bien. Si se prefiere GDScript, hay dos alternativas: una GDExtension en Rust (godot-rust/gdext con las crates `sysinfo` y `notify`) o un parser HTTP mínimo sobre `TCPServer` ([addon REST](https://godotengine.org/asset-library/asset/4167)).
2. **Los LLM confunden Godot 3 con Godot 4** (`yield`/`await`, `onready`/`@onready`, `KinematicBody2D`), y editar `.tscn` a mano es frágil. *Mitigación:* escribir un CLAUDE.md con las reglas de la 4.x, usar tipado estático, validar con `godot --headless --check-only` en el bucle del agente y armar la escena por código en lugar de editar `.tscn` complejos.
3. **No es una plataforma de UI web**: no hay npm de componentes, la accesibilidad y la composición de texto son más limitadas, y el `.pck` más las plantillas pesan decenas de MB. *Mitigación:* la UI de esta app (lista de agentes, panel de log, semáforo) cabe de sobra en `Control`, y la terminal futura está cubierta por GodotXterm. El tamaño baja con plantillas de export personalizadas.

## Ataques previstos
**Ataque 1 (Tauri/Electron/VS Code): "Godot es un motor de juegos y no sirve para una herramienta: paneles, texto, listas, terminal."**
Respuesta: el contraejemplo más fuerte es el editor de Godot, una herramienta compleja con docking, inspector, editor de código y depurador, hecha con los mismos nodos. Pixelorama sigue la versión 4.7.1. La terminal ya existe como GDExtension con PTY. Los rivales tienen el problema contrario y mayor: con PixiJS hay que integrar a mano tilemap (pixi-tilemap o Tiled), viewport, animación y loop, y luego sincronizar estado entre el DOM y el canvas. En Godot el mapa *es* la UI: cada oficina es una escena con su `Area2D`, su semáforo y sus `Control` hijos.

**Ataque 2 (Tauri): "La integración con el sistema es el 20% del puntaje y la de Godot es débil; Rust y Node la tienen de serie."**
Respuesta: es cierto para GDScript, por eso propongo C#. .NET tiene `Process`, `FileSystemWatcher` y `HttpListener` multiplataforma, al mismo nivel que Node, y sin el puente IPC JS↔Rust que exige Tauri. Además el trabajo real es modesto: los hooks de Claude Code se configuran como comandos (`curl -X POST 127.0.0.1:PORT`), detectar procesos es un sondeo cada 1-2 s (`ps`/`/proc`/`Get-Process`) y el semáforo es una máquina de estados. El SIGTERM elegante es un `Process.Start("kill", "-TERM pid")` con SIGKILL de respaldo.

*(Contra Flutter+Flame: Flame obliga a construir a mano la cámara, el tilemap y las herramientas que Godot trae con editor. Contra la extensión de VS Code: deja la app atada a un IDE y no cumple el "escritorio primero".)*

## Conclusión
Si el diferencial del producto es la experiencia "oficina viva" con pixel art, cámara y animaciones, Godot reduce el riesgo en el 15% más difícil de replicar (2D de juego) y cubre la parte de sistema con C#. Su punto débil real es el soporte de LLMs, que se mitiga con disciplina de prompts y validación headless. Propongo como prueba un prototipo de una semana que mida RAM y CPU en idle con 30 oficinas animadas.
