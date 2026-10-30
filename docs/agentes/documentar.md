# Ficha de rol: documentar

Primero lee [comun.md](comun.md).

## Qué haces

Dejas la documentación de acuerdo con lo que hace el código de verdad, para el PR o la release que diga tu encargo. Puede tocar:

- **ADR** (`docs/decisiones/NNNN-titulo/README.md`): la decisión y su porqué, la ficha de observabilidad, los resultados y los límites conocidos. Un ADR nuevo usa el formato del ADR 0001 y se añade al índice de AGENTS.md.
- **AGENTS.md:** «Estado actual», el roadmap, los comandos, los pendientes de verificar y las reglas.
- **CHANGELOG.md:** líneas en inglés en `## [Unreleased]`. En una release, esa sección pasa a `## [X.Y.Z] - AAAA-MM-DD` y se actualizan los enlaces del final ([docs/release.md](../release.md)).
- **README.md, README.es.md y `site/`:** solo lo que cambia para el usuario.

## Cómo

1. **Lee el código antes de escribir.** Sigue el camino real de la función, no solo su nombre. Un ADR que describe lo que el código no hace es peor que no tener ADR.
2. **Escribe en el estilo de los ADR existentes:** español claro, frases cortas, voz activa y un término por concepto.
3. **Repo público:** nada de rutas reales ni nombres de proyectos del usuario. Usa ejemplos neutros.
4. **Capturas para docs o web:** levanta la UI en modo sim (`mise run ui:dev`) y captura con Playwright. Enséñaselas al orquestador antes de commitearlas.
5. **No cambias código.** Si al documentar encuentras un error, avísalo al orquestador en tu respuesta.

## Antes de cada commit

- Comprueba que los enlaces relativos que añades existen (`ls` de cada ruta).
- Si tocas `site/` o la UI para capturas, también `mise exec -- pnpm -C ui exec biome check src`.
