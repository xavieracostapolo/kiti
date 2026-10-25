# ADR 0001: Stack tecnológico de Kiti

- **Fecha:** 2026-10-24
- **Estado:** Aceptada
- **Decisión:** Tauri v2 + backend Rust + UI TypeScript con PixiJS v8 y pixi-viewport. Solo macOS por ahora; Windows después.

## Contexto

Kiti es una app de escritorio con estética de videojuego 2D cenital (pixel art). Muestra una oficina virtual donde cada sala es un agente de código en ejecución: Claude Code, Codex CLI o Gemini CLI. Tiene que permitir zoom y pan, mostrar un semáforo de estado por agente y cerrar agentes. Es una herramienta que queda abierta todo el día.

## Proceso seguido

Se hizo un debate con agentes simulados para no decidir por intuición:

1. **Brief común** con requisitos y criterios ponderados: [debate/brief.md](debate/brief.md).
2. **Ronda 1 (apertura).** Cinco abogados, uno por candidato, investigaron el estado actual de su tecnología y defendieron su posición:
   - Tauri v2 + PixiJS: [r1_tauri.md](debate/r1_tauri.md)
   - Electron + PixiJS/Phaser: [r1_electron.md](debate/r1_electron.md)
   - Godot 4: [r1_godot.md](debate/r1_godot.md)
   - Flutter + Flame: [r1_flutter.md](debate/r1_flutter.md)
   - Extensión de VS Code: [r1_vscode.md](debate/r1_vscode.md)
3. **Ronda 2 (réplica).** Cada abogado atacó a los rivales e hizo concesiones: archivos `debate/r2_*.md`.
4. **Juez independiente.** Verificó los hechos disputados con fuentes y puntuó por su cuenta: [debate/veredicto.md](debate/veredicto.md).

## Resultado del juez (brief multiplataforma: macOS + Windows + Linux)

| Candidato | Ponderado |
|---|---|
| Electron | 8.25 |
| Tauri v2 | 7.75 |
| Godot 4 (C#) | 6.85 |
| Extensión VS Code | 6.80 |
| Flutter + Flame | 6.60 |

El juez eligió Electron sobre todo por dos razones: Chromium es idéntico en todos los SO, y en Linux WebKitGTK es un riesgo para WebGL. También explicó cuándo ganaría Tauri:

- si el consumo en reposo es un requisito duro,
- si Linux queda fuera del alcance,
- o si se acepta Rust en el backend.

## Por qué la decisión final es Tauri

Después del debate, el usuario definió las condiciones reales del producto:

- **Plataforma:** solo macOS ahora, Windows más adelante. Linux no está previsto. En macOS Tauri usa WKWebView y WebGL funciona bien. En Windows usa WebView2 (Chromium). El riesgo de WebKitGTK desaparece.
- **Prioridad:** el rendimiento con 20 o más agentes simultáneos, de modo que la app vaya "como si nada".
- **Backend:** se acepta Rust.

Las tres condiciones del juez a favor de Tauri se cumplen. Recalculando solo para macOS, Tauri sube a ~8.05 frente a 8.25 de Electron, casi un empate. La prioridad de rendimiento decide a favor de Tauri: menos RAM, binario más pequeño y backend nativo con `sysinfo`.

## Alternativa considerada solo para macOS

Una app nativa en Swift (SwiftUI + SpriteKit) habría sido la opción más eficiente si el producto fuera exclusivo de macOS para siempre. Su estimación rápida fue de ~9.0, pero no pasó por debate. Se descartó porque Windows está en el plan y eso obligaría a reescribir todo.

## Hechos verificados que influyen en el diseño

Todos fueron verificados por el juez; las fuentes están en `veredicto.md`.

- `sysinfo` lee el `cwd` de procesos ajenos en macOS (`proc_pidinfo`) y en Windows (PEB). Si falla, devuelve `None` sin avisar.
- En Windows no existe un SIGTERM real: `sysinfo` usa `taskkill /F`.
- Claude Code es un binario nativo y Codex CLI está escrito en Rust. Gemini CLI corre sobre Node, así que su proceso aparece como `node`. **Hay que detectar los agentes por la línea de comando, no por el nombre del proceso.**
- Claude Code admite hooks de tipo `http` y envía `cwd`, `session_id` y `transcript_path`. Codex y Gemini solo admiten hooks de tipo `command`, y los ejecutan de forma síncrona.
- El fallback Canvas de PixiJS 8.16 es experimental y no se activa solo.

## Cuándo reabrir esta decisión

Solo si cambia alguna de estas condiciones:

- Linux entra en el alcance.
- El equipo decide abandonar Rust.
- El producto se vuelve más juego que herramienta (avatares con pathfinding, iluminación, partículas). En ese caso hay que evaluar Godot.
- Tauri v3 con runtime CEF madura y cambia el equilibrio.
