# ADR 0013: Oficinas en pixel art con tiles de Kenney

- **Fecha:** 2026-10-25
- **Estado:** Aceptada
- **Decisión:** la sala de cada oficina se dibuja con tiles de **Kenney (CC0, 16×16 px)** en lugar de rectángulos de colores. Se usan tres packs del mismo autor:

  | Pack | Uso |
  |---|---|
  | Roguelike RPG Pack | Pared, suelo y alfombras |
  | Roguelike Indoors | Muebles |
  | RPG Urban Pack | Personas |

  Lo funcional sigue dibujado por código: semáforo, pantalla del monitor, título, insignias, nota, cartel y bordes.

## Contexto

Desde el Hito 1 la oficina era procedural (`officeTemplate.ts`). El ADR 0006 ya preveía Kenney. Antes de decidir se compararon los packs y se hizo una maqueta, que el usuario aprobó: suelo de madera, una persona por agente, una alfombra del color del proveedor y oficinas apagadas sin nadie.

## Packs revisados (descargados y abiertos, no de memoria)

| Pack | Licencia | Tile | Veredicto |
|---|---|---|---|
| [Roguelike RPG Pack](https://kenney.nl/assets/roguelike-rpg-pack) | CC0 | 16×16, 1 px de separación | **Sí:** pared, suelo y alfombras. Los ids salen de su `sample_indoor.tmx` |
| [Roguelike Indoors](https://kenney.nl/assets/roguelike-indoors) | CC0 | 16×16, 1 px | **Sí:** escritorio, silla, estantería, cuadro y plantas. Trae solo muebles |
| [RPG Urban Pack](https://kenney.nl/assets/rpg-urban-pack) | CC0 | 16×16, 1 px | **Sí:** 6 personas modernas en 4 direcciones; se usa la de espaldas (columna 25) |
| [Roguelike Characters](https://kenney.nl/assets/roguelike-characters) | CC0 | 16×16 | No: estética medieval |
| [Tiny Town](https://kenney.nl/assets/tiny-town), [1-Bit Pack](https://kenney.nl/assets/1-bit-pack) | CC0 | 16×16 | No: exteriores y monocromo |

Ningún pack trae ordenadores cenitales de 16 px, así que el monitor sigue siendo procedural. Además, su pantalla lleva el color del estado.

## Diseño

- **Assets:** las tres hojas PNG (152 KB en total) y sus `License.txt` están en `ui/public/assets/kenney/<pack>/`. Vite las copia tal cual al build.
- **Atlas:** `world/kenney.ts` define las coordenadas (columna, fila) de cada tile y carga las hojas con `Assets.load`. Los tiles son vistas (`frame`) sobre la misma textura y no copian píxeles.
- **Una plantilla por proveedor:** `officeTemplate.ts` compone la sala con sprites y se rasteriza **una vez por proveedor** (solo cambia la alfombra: naranja para Claude, verde para Codex y turquesa para Gemini). Todas las oficinas de ese proveedor comparten la textura, como hasta ahora.
- **Persona:** `domain/people.ts` (puro, FNV-1a) elige una de las 6 personas por el id del agente: siempre la misma para el mismo agente. Es un `Sprite` sobre la silla. Las oficinas apagadas no tienen persona: queda la silla vacía y la sala en penumbra.
- **Lo que no cambia:** anclas y elementos funcionales, LOD, culling, filtro, zonas y textos.

## Créditos

CC0 no obliga a citar, pero el README acredita a Kenney (www.kenney.nl).

## Verificación

- `people.ts` tiene tests: estable, reparte entre las 6 personas y siempre devuelve un índice dentro del rango.
- E2E en Chromium, con la CSP de producción incluida: las hojas se cargan desde `'self'`.
- Se regeneraron las líneas base visuales y se revisaron las capturas en los zooms lejano, medio y cercano.
