# fixtures/sim

Datos compartidos por los dos simuladores de agentes (ADR 0006 §9):

- `ui/src/transport/sim/` (TypeScript, Hito 1), para la UI en el navegador y en CI.
- `SimFleet` en `crates/core` (Rust, Hito 2), para tests, benches y la demo en Tauri.

`profiles.json` define los parámetros de generación: proveedores, herramientas, proyectos y perfiles de comportamiento con sus medias.

`conformance/*.json` son secuencias de señales normalizadas (los 8 eventos del ADR 0005 más `ProcessGone`, que viene del escaneo de procesos) con el estado esperado tras cada una. Los dos simuladores, y el reducer real del core, deben pasar todos los casos. Si un archivo tiene `then`, es una segunda secuencia que empieza con un agente nuevo.

Cada implementación es determinista con su semilla, pero no se exige la misma secuencia entre lenguajes.
