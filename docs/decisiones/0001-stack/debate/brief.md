# Debate técnico: framework para "Agent Office"

## Producto
App de escritorio visual, estilo videojuego 2D top-down (pixel art, tipo Gather.town / Stardew).
Muestra una "oficina" virtual donde cada oficina (room) = un agente de IA de código ejecutándose
en la máquina del usuario (Claude Code CLI, Codex CLI, Gemini CLI). Objetivo: centralizar todo.

## Requisitos funcionales (MVP y cercanos)
- Ver solo agentes en ejecución (detectar procesos `claude`, `codex`, `gemini`: PID, cwd, hora inicio).
- Cámara: zoom in a la oficina de un agente, zoom out para ver todas; pan.
- Semáforo por oficina: verde trabajando, amarillo esperando input/permiso, rojo error/muerto, gris idle.
- Cerrar oficina = terminar proceso del agente (SIGTERM/SIGKILL, equivalente en Windows).
- Recibir eventos de estado: servidor HTTP local (127.0.0.1) que recibe hooks de Claude Code
  (SessionStart, PreToolUse, PostToolUse, Notification, Stop...), notify de Codex, hooks de Gemini CLI.
- Leer/tail de transcripts JSONL (~/.claude/projects, ~/.codex/sessions, ~/.gemini/tmp) como respaldo.
- Se agregarán features con el tiempo (paneles laterales, logs, posiblemente lanzar agentes, terminal embebida, etc.).

## Requisitos no funcionales
- Escritorio primero, fácil de volver multiplataforma (macOS, Windows, Linux).
- Desarrollador: un solo dev, usa mucho agentes de IA para programar (importa que el stack sea bien
  conocido por LLMs y tenga buen ecosistema).
- Rendimiento razonable con 1-30 agentes simultáneos; app que puede quedar abierta todo el día (consumo RAM/CPU importa).
- Mezcla de "juego" (sprites, animaciones, tilemap, cámara) y "herramienta" (UI de paneles, texto, listas, forms).

## Candidatos
1. Tauri v2 + TypeScript + PixiJS (+ pixi-viewport)
2. Electron + TypeScript + PixiJS/Phaser
3. Godot 4 (GDScript o C#)
4. Flutter desktop + Flame
5. Extensión de VS Code (webview + PixiJS)

## Criterios de evaluación (peso)
- Integración con sistema: procesos, señales, sockets, file watching (20%)
- Capacidad 2D de juego: tilemap, sprites, cámara/zoom, animación (15%)
- UI tipo herramienta: paneles, texto, listas, terminal embebida futura (15%)
- Multiplataforma real y empaquetado/distribución/firma (15%)
- Consumo de recursos para app always-on (10%)
- Velocidad de desarrollo para un solo dev + soporte de LLMs/ecosistema (15%)
- Madurez, comunidad y riesgo a largo plazo (10%)
