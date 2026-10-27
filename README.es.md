<div align="center">

# Kiti

**Una oficina virtual en pixel art donde cada sala es un agente de código que corre en tu Mac.**

Claude Code, Codex CLI, Antigravity CLI y Gemini CLI en un solo mapa, con un semáforo por agente.

[![CI](https://github.com/xavieracostapolo/kiti/actions/workflows/ci.yml/badge.svg)](https://github.com/xavieracostapolo/kiti/actions/workflows/ci.yml)
[![Licencia: MIT](https://img.shields.io/badge/licencia-MIT-blue.svg)](LICENSE)
![Plataforma: macOS](https://img.shields.io/badge/plataforma-macOS-lightgrey.svg)
![Tauri v2](https://img.shields.io/badge/Tauri-v2-24c8db.svg)

[**Descargar para macOS**](https://github.com/xavieracostapolo/kiti/releases/latest/download/Kiti_universal.dmg) · [**Prueba la demo**](https://xavieracostapolo.github.io/kiti/demo/) · [Web](https://xavieracostapolo.github.io/kiti/) · [Read in English](README.md)

![Kiti con 20 agentes simulados agrupados por proyecto, cada oficina con su título y su semáforo](site/img/map.png)

</div>

## Por qué

Cuando tienes varios agentes de código abiertos, les pierdes la pista. Uno espera un permiso, otro terminó hace diez minutos y un tercero se cayó. Kiti vigila los agentes de tu máquina y dibuja cada uno como una oficina. Ves a todo el equipo de un vistazo y te acercas al que te necesita.

## Funciones

- **Detección automática.** Kiti encuentra los procesos de Claude Code, Codex CLI, Antigravity CLI (`agy`) y Gemini CLI. Los clasifica por ejecutable y `argv`, nunca por el nombre del proceso, e ignora los lanzadores y las apps de escritorio.
- **Semáforo por agente.** Verde es trabajando, amarillo es esperándote, gris es inactivo y rojo es error o caída. El estado sale de los hooks de los CLIs, no de adivinar. Un fallo de herramienta no es rojo.
- **Zonas por proyecto.** Las oficinas se agrupan por raíz git (o carpeta): cada repositorio es un barrio.
- **Cámara.** Pan, zoom, doble clic para enfocar una oficina y `0` para verlo todo. De lejos, cada oficina muestra su título en un cartel grande; de cerca ves el título, la herramienta actual y el último pedido.
- **Filtro y buscador.** Los contadores de la barra superior filtran por estado, también las conversaciones apagadas. Pulsa `/` para buscar por título, proyecto, carpeta o proveedor, y `Esc` para limpiar. Las oficinas que no coinciden se atenúan, no se esconden: el mapa conserva su orden y la cámara encuadra las que coinciden.
- **Terminal embebida.** Lanza un agente nuevo desde Kiti. Corre en `tmux` (`-L kiti`), así que sobrevive cuando cierras Kiti.
- **Ir a su terminal.** Para un agente abierto fuera de Kiti, «Ir a su terminal» muestra su pestaña exacta en Ghostty o Terminal.app. Otras apps, como VS Code, pasan al frente.
- **Retomar conversaciones.** Las conversaciones cerradas de Claude de los últimos 7 días quedan como oficinas apagadas. Un clic ejecuta `claude --resume` en una terminal de Kiti.
- **Barra de menú y avisos.** La barra de menú muestra los contadores. macOS te avisa cuando un agente pide permiso, termina un turno largo, falla o se cae, solo si Kiti no está al frente. Un clic en el aviso abre Kiti en esa oficina.
- **Cuota de uso.** La barra superior muestra cuánto queda de tu plan de Claude (ventanas de 5 h y semanal) y de tu cuota de Antigravity, y Kiti avisa una vez cuando a un límite semanal le queda el 20 % o menos.
- **Tablero de la oficina.** Al acercarte a una oficina ves el modelo, el contexto usado y el coste de la sesión (Claude), la rama de git del proyecto, sus cambios y los commits por subir o por bajar, y una línea de tiempo de la última hora. Solo aparece en el zoom más cercano, para no recargar la vista general.
- **Pixel art.** Salas, muebles y personas salen de los tilesheets CC0 de [Kenney](https://kenney.nl). Cada agente tiene siempre la misma persona, y la alfombra lleva el color de su proveedor.
- **Cerrar una oficina.** Termina el proceso del agente tras una confirmación. Kiti verifica `(pid, start_time, uid, cmd)` antes de cada señal, así que nunca mata un PID reutilizado.

<table>
<tr>
<td><img src="site/img/board.png" alt="Oficina de Claude de cerca con su tablero: modelo, contexto, coste de la sesión, estado de git y la última hora; la barra superior muestra la cuota de Claude y Antigravity"></td>
<td><img src="site/img/filter.png" alt="Filtro «esperando» activo: las dos oficinas amarillas destacan y el resto se atenúa"></td>
</tr>
<tr>
<td><img src="site/img/external.png" alt="Panel de un agente abierto fuera de Kiti, con el botón para ir a su terminal"></td>
<td><img src="site/img/dormant.png" alt="Conversación cerrada como oficina apagada con la silla vacía, y el panel para retomarla"></td>
</tr>
</table>

## Hecho para muchos agentes

El rendimiento es la prioridad número uno. El objetivo es 20 agentes o más sin coste visible.

| Medida | Resultado |
|---|---|
| CPU en reposo | 0,06 % |
| RAM en reposo | 197 MB (unos 158 MB son la base de WebKit y WebGL) |
| RAM con 20 / 50 agentes activos | 274 MB / 288 MB |
| Frames cuando nada se mueve | 0 (render bajo demanda) |

Los transcripts se leen de forma incremental desde el último offset, el core envía diffs agregados a unos 10 Hz y las oficinas fuera de cámara no se dibujan. El método está en el [ADR 0006](docs/decisiones/0006-arquitectura-consolidada/README.md#resultados-del-hito-2).

## Privacidad y seguridad

Todo se queda en tu máquina. Kiti no tiene servidor ni telemetría.

- El servidor de hooks escucha solo en `127.0.0.1` y exige un token por instalación. Comprueba `Host` y `Origin`, limita el tamaño del cuerpo y siempre responde `204`: Kiti nunca devuelve una decisión a un agente.
- Los prompts, el código y las salidas de herramientas nunca se registran ni se guardan. El título y el último pedido solo viajan del core a la UI.
- Al primer arranque, Kiti añade sus hooks HTTP a `~/.claude/settings.json` y deja intactos los tuyos. `mise run hooks:uninstall` los quita. Los hooks de Codex, Antigravity y Gemini son opcionales.
- La cuota y el tablero de Claude necesitan que su status line pase por Kiti. Es opcional: `mise run statusline:install` pone `kiti-hook` delante de tu comando de status line, que sigue mostrando lo mismo, y `mise run statusline:uninstall` lo restaura. De ese JSON Kiti solo se queda con los límites de uso, el modelo, el contexto y el coste.
- Si `agy` está instalado, Kiti ejecuta `agy -p /usage --output-format json` al arrancar y cada 5 minutos para leer la cuota de Antigravity. La llamada no gasta tokens.
- Cada 15 s, Kiti ejecuta `git --no-optional-locks status` en los proyectos con agentes vivos, así que nunca les quita el `index.lock`.

Para reportar vulnerabilidades, sigue [SECURITY.md](SECURITY.md).

## Primeros pasos

Kiti funciona en macOS. Windows está previsto; Linux no.

### Descarga

[Descarga `Kiti_universal.dmg`](https://github.com/xavieracostapolo/kiti/releases/latest/download/Kiti_universal.dmg) (último release), o elige una versión en [Releases](https://github.com/xavieracostapolo/kiti/releases). Funciona en Apple Silicon e Intel. Abre el DMG y arrastra Kiti a Aplicaciones.

El release tiene **firma ad-hoc**: no está firmado con un Developer ID de Apple ni notarizado, así que macOS lo bloquea la primera vez. Abre Kiti una vez y pulsa **Abrir igualmente** en **Ajustes del Sistema → Privacidad y seguridad**. O ejecuta:

```bash
xattr -dr com.apple.quarantine /Applications/Kiti.app
```

macOS puede volver a pedir los permisos de Automatización y de notificaciones tras cada actualización. Si compilas desde el código no hace falta nada de esto.

### Pruébalo en el navegador

La UI funciona sin la app nativa, con agentes simulados:

```bash
git clone https://github.com/xavieracostapolo/kiti.git && cd kiti
brew install mise rustup tmux          # tmux es opcional, para sesiones persistentes
mise install && mise exec -- pnpm install
mise run ui:dev                        # http://127.0.0.1:5173/?sim=20&seed=42
```

O abre la [demo](https://xavieracostapolo.github.io/kiti/demo/). Parámetros de URL: `sim` (número de agentes), `seed`, `conversations=1` (conversaciones cerradas), `terminals=1` (terminal falsa) y `quota=1` (cuota falsa).

### Ejecuta la app

En Homebrew, `rustup` es *keg-only*: añade antes `/opt/homebrew/opt/rustup/bin` al `PATH`. `rust-toolchain.toml` y `mise.toml` fijan las versiones.

```bash
mise run app:dev       # app nativa con los agentes reales de esta máquina
mise run app:sim       # app nativa con 20 agentes simulados
mise run app:bundle    # genera Kiti.app y la copia al Escritorio
```

### Conecta los CLIs

| CLI | Cómo escucha Kiti | Configuración |
|---|---|---|
| Claude Code | Hooks `http` nativos y `~/.claude/sessions/<pid>.json` | Automática al primer arranque |
| Codex CLI | Hooks `command` a través del forwarder `kiti-hook` | `mise run hooks:install-codex` y luego confía en el hook en `/hooks` |
| Antigravity CLI | Hooks `command` a través del forwarder `kiti-hook`, y su base SQLite local para los permisos y los títulos | `mise run hooks:install-antigravity` |
| Gemini CLI | Hooks `command` a través del forwarder `kiti-hook` | `mise run hooks:install-gemini` |

Para la cuota y el tablero de Claude, ejecuta también `mise run statusline:install` (ver [Privacidad y seguridad](#privacidad-y-seguridad)).

`mise run hooks:status` muestra qué está instalado. `mise run scan` muestra qué procesos ve Kiti como agentes y por qué. El forwarder falla en abierto: tiene un plazo de 150 ms, siempre sale con `0` y no hace nada si Kiti está cerrado.

## Cómo funciona

```
[claude]  --hook http----------------> [core: HookServer, 127.0.0.1]
[codex]   --hook command--> [kiti-hook] --> [core: HookServer]
[gemini]  --hook command--> [kiti-hook] --> [core: HookServer]
[agy]     --hook command--> [kiti-hook] --> [core: HookServer]
[procesos] --sysinfo cada 2 s--------> [core: ProcessScanner]
[~/.claude/sessions/*.json] --notify-> [core: ClaudeSessionsWatcher]
[transcripts *.jsonl] --FSEvents-----> [core: TranscriptTailer]
                                          |
                          [core: AgentRegistry + reducer del semáforo]
                                          |  diffs agregados, ~10 Hz
                          [src-tauri: tauri::ipc::Channel]
                                          |
                          [ui: store -> mundo PixiJS + paneles React]
```

| Ruta | Contenido |
|---|---|
| `crates/protocol` | DTOs compartidos, exportados a TypeScript con `ts-rs` |
| `crates/core` | Dominio, aplicación, ports y adapters en Rust puro. Sin Tauri |
| `crates/forwarder` | `kiti-hook`, el forwarder de los hooks de Codex, Antigravity y Gemini |
| `src-tauri` | Capa fina: commands, canal, capabilities y bandeja |
| `ui` | Vite, TypeScript y PixiJS v8 con cámara propia; React solo para paneles |
| `docs/decisiones` | Registros de decisiones de arquitectura |

Stack: Tauri v2, Rust (`tokio`, `axum`, `sysinfo`), TypeScript 7, PixiJS v8, React, zustand y xterm.js. Tests: Vitest, fast-check y Playwright en la UI; nextest, proptest e insta en Rust.

El diseño está en ADRs, desde la [elección del stack](docs/decisiones/0001-stack/README.md) hasta el [pixel art](docs/decisiones/0013-pixel-art-kenney/README.md). [AGENTS.md](AGENTS.md) es la guía completa del código.

## Roadmap

- [x] Oficina simulada, shell nativo, procesos reales y hooks de Claude Code, Codex y Gemini
- [x] Tailer de transcripts, barra de menú, terminal embebida y sesiones persistentes en `tmux`
- [x] Zonas por proyecto, conversaciones para retomar y avisos
- [x] Vista lejana legible, filtro de oficinas e ir a la pestaña de terminal de un agente externo
- [x] Pixel art con los tilesheets de [Kenney](https://kenney.nl)
- [x] Antigravity CLI, el sucesor de Gemini CLI para cuentas gratuitas
- [x] Build universal descargable (firma ad-hoc)
- [x] Cuota de uso de Claude y Antigravity, y tablero de oficina con sesión, git y la última hora
- [ ] Soporte para Windows
- [ ] Versiones firmadas y notarizadas

## Contribuir

Las contribuciones son bienvenidas. Lee antes [CONTRIBUTING.md](CONTRIBUTING.md): el proyecto usa TDD y Conventional Commits.

## Licencia

[MIT](LICENSE) © 2026 Xavier Acosta Polo

El arte de las oficinas es de [Kenney](https://www.kenney.nl) (Roguelike RPG Pack, Roguelike Indoors y RPG Urban Pack), con licencia [CC0](https://creativecommons.org/publicdomain/zero/1.0/deed.es). Cada pack conserva su archivo de licencia en [`ui/public/assets/kenney/`](ui/public/assets/kenney/).
