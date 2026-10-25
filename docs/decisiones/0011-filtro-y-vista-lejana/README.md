# ADR 0011: Filtro de oficinas y vista lejana legible

- **Fecha:** 2026-10-25
- **Estado:** Aceptada
- **Decisión:** En la vista lejana cada oficina muestra su título en un cartel grande. Los contadores de la barra superior filtran por estado (también las oficinas apagadas), y un buscador filtra por texto. Las oficinas que no coinciden **se atenúan, no se esconden**, y la cámara encuadra las que coinciden.

## Contexto

Con muchas oficinas, encontrar la que te necesita es lento. Desde lejos no se leía nada: el título solo aparecía al acercarse. Los avisos del ADR 0010 dicen *que* algo pasó; hacía falta llegar rápido a *dónde*.

## Decisiones

- **Cartel en la vista lejana:** el título (o la carpeta) en el centro de la sala, hasta 3 líneas. La letra se adapta al zoom (`fontFor`, ADR 0009) y baja lo necesario para que la palabra más larga quepa entera (`fitWords`). En `mid` y `near` desaparece y vuelven el título de la pared y la nota.
- **Filtro** (`state/filter.ts`, puro): estados del semáforo más `dormant`, combinables, y un texto que busca en título, proyecto, carpeta y proveedor, sin mayúsculas ni tildes.
- **Barra superior:**
  - Cada contador es un botón con `aria-pressed`. Hay un contador de apagadas cuando las hay, y un botón «Quitar filtro».
  - Buscador: `/` lo enfoca y `Esc` lo limpia.
  - Se quitó la línea de ayuda de la cámara para hacer sitio.
- **Atenuar en lugar de esconder:** el mapa no se reordena, así que se conserva la memoria espacial y no cambian los slots. Al cambiar el filtro, la cámara encuadra la unión de las oficinas que coinciden; al quitarlo, vuelve a la vista general.

## Verificación

- Vitest: filtro, `fitWords`, `union` y la barra (contadores, `/` y `Esc`).
- E2E: «esperando» atenúa el resto y acerca la cámara; el buscador encuentra por proyecto, también las apagadas.
- El test de reposo sigue en 0 frames: atenuar solo cambia `alpha` cuando cambia el filtro o los agentes.
- Rendimiento (50 agentes, WebKit): el coste por frame en p95 pasa de 1 a 2 ms (límite: 8). El p95 entre frames dio 17-18 ms tanto en esta rama como en `main` con el Mac cargado (load average 3-5): está en el límite de la resolución del timer y no es una regresión. Los textos ocultos (el cartel de cerca; el título y la nota de lejos) no se recalculan con el zoom: se marcan y se rehacen al volver a verse.
