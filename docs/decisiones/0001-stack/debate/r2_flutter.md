# Ronda 2 — Réplica de Flutter desktop + Flame

## 1. Errores factuales sobre mí
Tres rivales afirman cosas que se pueden comprobar en la documentación oficial:
- **Tauri**: "la integración de sistema exige FFI o plugins". Falso. `Process.start`, `Process.killPid(pid, ProcessSignal.sigterm)`, `HttpServer.bind` y `FileSystemEntity.watch` vienen en `dart:io` del SDK estándar ([killPid](https://api.dart.dev/stable/dart-io/Process/killPid.html), [watch](https://api.dart.dev/stable/dart-io/FileSystemEntity/watch.html)). Hace falta FFI solo para *listar* procesos ajenos con su cwd, y Electron, Godot y VS Code también tienen que recurrir a `ps`/`lsof`/PowerShell para eso.
- **Godot**: "Flame obliga a construir a mano cámara y tilemap". Falso. Hay `CameraComponent` con `Viewfinder`, `follow`, `setBounds` y zoom por `ScaleEffect` ([docs](https://docs.flame-engine.org/latest/flame/camera.html)), y `flame_tiled` es un paquete oficial. Lo que no hay es editor visual de escenas; Tiled cumple ese papel.
- **VS Code**: "procesos y sockets sin Node". No hace falta Node: Dart tiene servidor HTTP y procesos de serie.

## 2. Afirmaciones débiles o exageradas de cada rival
**Tauri (7,9)**
- Pone un 9 en integración, pero en Windows `sysinfo::kill_with` devuelve `None` para cualquier señal que no sea `Kill`, y `Kill` es "la única soportada en todas las plataformas" ([docs.rs](https://docs.rs/sysinfo/latest/sysinfo/struct.Process.html)). Es el mismo límite que tiene Dart, así que no justifica un punto más que yo.
- Pone un 9 en UI y a la vez reconoce que en WebKitGTK hay ventanas en blanco con NVIDIA y que WebGL puede caer a un camino lento sin avisar. Con tres motores web distintos no se llega a un 9. Además, la terminal futura obliga a pasar los bytes del PTY de Rust a xterm.js por IPC, una capa más que en Electron.
- En recursos se pone un 8, pero cita 109 MB contra 128 MB de Electron. Esa diferencia no da para 3 puntos de distancia.

**Electron (8,75)**
- La tesis de que "monitoriza CLIs de Node" no se sostiene. Codex se reescribió en Rust en 2025 ([InfoQ](https://www.infoq.com/news/2025/06/codex-cli-rust-native-rewrite)) y Claude Code se instala como binario nativo que no invoca Node ([docs](https://code.claude.com/docs/en/getting-started/installation)). Da igual en qué esté escrito el agente: se le monitoriza por PID, hooks HTTP y JSONL.
- Se pone tres dieces/nueves (UI 10, LLMs 10, distribución 9). Un 10 en UI significa "no se puede mejorar", y la propia app reconoce entre 100 y 300 MB en reposo y módulos nativos (node-pty) que hay que recompilar en cada major.
- Su propia debilidad 1 (RAM) cae en el único criterio que penaliza tenerla abierta todo el día, que es justo el caso de uso.

**Godot (7,4)**
- La integración depende de C#, así que serían dos lenguajes (GDScript + C#) más el runtime .NET. Ese es el mismo problema de "dos lenguajes" que le critican a Tauri.
- Se pone un 7 en UI, pero tiene que remitir a GodotXterm (una GDExtension de terceros) y a widgets `Control` sin ecosistema de componentes. Para paneles densos en texto, Flutter está claramente por encima.

**Extensión de VS Code (8,2)**
- El 9 en distribución esconde que el producto no existe sin el editor abierto. El brief pide "escritorio primero" y "always-on".
- Elegir un líder entre N extension hosts por un puerto fijo es una fuente de bugs permanente, no un detalle.

## 3. Concesiones honestas
- **Godot me gana en always-on**: su low processor mode redibuja solo cuando hace falta y viene de serie ([docs OS](https://docs.godotengine.org/en/stable/classes/class_os.html)). En Flame tengo que programar yo `pauseEngine()`/`resumeEngine()`. La UI de Flutter sí redibuja solo lo que se invalida, pero el `GameWidget` funciona con ticker continuo. Godot también me gana en 2D (tiene editor).
- **Electron me gana en terminal**: node-pty + xterm.js es el par de VS Code, mientras que `flutter_pty` 0.4.2 lleva unos 21 meses sin publicar ([pub.dev](https://pub.dev/packages/flutter_pty)).
- **Tauri y Electron me ganan en updater y firma integrados**. Bajo **Multiplataforma de 7 a 6**: no tengo bundler oficial de instaladores ni updater de primera parte.
- **Pixel Agents** ([GitHub](https://github.com/pablodelucca/pixel-agents)) demuestra que el concepto funciona con TS + Pixi. Es una prueba real a favor del stack web.
- Mantengo un **6 en LLMs**.

Nueva media ponderada: **≈6,95**.

## 4. Por qué sigo siendo una opción defendible
Aunque concedo puntos, Flutter es el único candidato con **un solo lenguaje, un solo proceso nativo y sin webview** que saca nota alta (8) tanto en sistema como en UI. Tauri necesita Rust + TS, Godot necesita GDScript + C#, Electron paga en RAM y VS Code depende del editor. Mi riesgo principal es que los LLMs conocen poco Flame, y eso se compensa con lints estrictos y un `CLAUDE.md` con la versión fijada.
