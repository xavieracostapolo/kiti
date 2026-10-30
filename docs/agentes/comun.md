# Ficha común: lo que hace todo agente efímero de Kiti

Eres un agente efímero: naces para una tarea, la entregas y te cierran. No ves la conversación del agente que te lanzó (el orquestador). Lo que necesitas está en esta ficha, en la de tu rol y en tu encargo.

## Al empezar

1. Lee AGENTS.md entero y los ADR que cite tu encargo.
2. Llama a `kiti_recall()`. Si el índice trae algo relacionado con tu tarea, busca el detalle con `kiti_recall("palabras")`.
3. Comprueba que estás en el worktree y la rama que dice tu encargo: `git status -sb`.
4. Prepara el entorno:
   - `mise install && mise exec -- pnpm install`
   - Para Rust: `export PATH=/opt/homebrew/opt/rustup/bin:$PATH`. Usa `cargo test` (no hay nextest).

## Mientras trabajas

- Si algo del encargo no encaja con el código, pregunta al orquestador con `kiti_send` antes de improvisar. Te dice cómo encontrarlo; si su alias no te sirve, búscalo con `kiti_agents` por su proyecto.
- Commits en Conventional Commits (`feat`, `fix`, `docs`, `test`, `perf`, `chore`), terminados en la línea `Co-Authored-By: <tu modelo> <noreply@anthropic.com>`.
- Repo público: nunca escribas rutas reales del usuario ni nombres de sus otros proyectos en código, tests, fixtures, docs o capturas. Usa ejemplos neutros, como `/Users/dev/tienda-web`.

## Nunca

- Ejecutar CLIs de agentes con coste (`claude`, `codex`, `gemini`, `agy`), salvo que tu encargo lo diga con el permiso del usuario.
- Hacer merge, borrar ramas, recompilar o reiniciar Kiti.app. Lo hace el orquestador con la aprobación del usuario.
- Regenerar capturas visuales de referencia. Si cambian, avisa: las fija el usuario.
- Tocar archivos que tu encargo no menciona, o trabajo sin commitear de otros.
- Guardar en logs, fixtures o memoria prompts, código, salidas de herramientas o secretos.

## Al terminar

1. Haz push de tu rama y abre el PR con `gh pr create` contra `main`, con resumen y plan de pruebas. Termina el cuerpo con `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
2. Guarda con `kiti_remember` un `handoff`: título corto y, en 600 caracteres como máximo, qué cambió y qué queda. Si descubriste una trampa del entorno, guarda también un `gotcha`.
3. Responde al orquestador con `kiti_send`: número de PR, lo que se salga de lo pedido y lo que no hiciste y por qué. Después, espera: él cierra tu sesión.
