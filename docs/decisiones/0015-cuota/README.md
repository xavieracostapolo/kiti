# ADR 0015: Cuota de uso de Claude y Antigravity

- **Fecha:** 2026-10-25
- **Estado:** Aceptada
- **Decisión:** Kiti muestra cuánta cuota queda en la barra superior y avisa cuando a un límite semanal le queda el 20 % o menos.
  - **Claude:** las ventanas de 5 h y semanal salen del JSON que Claude Code pasa a su *status line*.
  - **Antigravity:** los grupos de modelos y sus ventanas salen de `agy -p /usage --output-format json`.

## Contexto

El usuario quiere saber cuánto le queda de su plan en la semana sin salir de Kiti. Ninguno de los dos CLIs tiene un evento de hook para la cuota, pero los dos la exponen:

- **Claude Code** añade al JSON de la *status line* `rate_limits.five_hour` y `rate_limits.seven_day`, cada uno con `used_percentage` y `resets_at`. Solo lo hace con suscripción Pro o Max y después de la primera respuesta de la sesión ([documentación](https://code.claude.com/docs/en/statusline)).
- **Antigravity CLI** responde a `agy -p /usage --output-format json` con `command.data.groups[].buckets[]`, cada uno con `window` (`weekly` o `5h`), `remaining_fraction` y `reset_time` (RFC 3339). La llamada no gasta tokens: `usage.total_tokens = 0`.

Se descartó leer el token OAuth de Claude del llavero de macOS para consultar su API interna de uso: no está documentada y obliga a tocar credenciales.

## Decisiones (con el visto bueno del usuario)

- **Status line de Claude:** Kiti se pone delante del script del usuario.
  - `statusLine.command` pasa a ser `~/.kiti/bin/kiti-hook statusline '<comando original>'`.
  - El forwarder ejecuta el comando original con el mismo stdin e imprime su salida, así que la status line se ve igual. Después reenvía el JSON a Kiti (`POST /v1/statusline`).
  - El core se queda solo con `rate_limits`.
  - Se instala con copia de seguridad y se revierte con `mise run statusline:uninstall`, que restaura el comando original.
- **Antigravity:** si `agy` está instalado, el shell lo consulta al arrancar y cada 5 minutos. Ese proceso (`agy -p /usage`) no cuenta como oficina.
- **Aviso:** una vez por ventana semanal y periodo (es decir, por `resets_at`), cuando lo que queda baja al 20 % o menos. Las ventanas de 5 h no avisan, porque se reinician cada pocas horas.
- **Interfaz:** un componente aparte (`QuotaBadge`) en la barra superior: lo que queda de cada proveedor con un color, y el detalle al pasar el ratón.

## Diseño

- **`domain/quota.rs` (puro):**
  - `QuotaWindow { label, weekly, remaining, resets_at }` y `Quota { provider, windows }`.
  - `QuotaPolicy` decide los avisos.
- **ACLs puras:**
  - `ingestion::claude::parse_rate_limits`: JSON de la status line → ventanas.
  - `ingestion::antigravity::parse_usage`: JSON de `/usage` → ventanas.
- **Protocolo:** `ServerMsg::Quota { provider, windows }`. El runtime guarda la última cuota de cada proveedor y la reenvía tras cada Snapshot.
- **Privacidad:** del JSON de la status line solo se leen los dos `rate_limits`; el resto (carpeta, modelo, coste) se descarta en la ACL. No se registra nada.

## Ficha de observabilidad

| Pregunta | Señal | Verificación |
|---|---|---|
| ¿Llega la cuota de Claude? | `quota.updated` (debug) con `provider=claude` y el número de ventanas | Tras una respuesta en una sesión de Claude |
| ¿Funciona la consulta a agy? | `quota.updated` con `provider=antigravity`, o `quota.error` (warn) con el motivo | Al arrancar y cada 5 min |
| ¿Se avisa? | `alert.raised` / `alert.sent` con `kind=QuotaLow` | Test de la política; en real, cuando baje del 20 % |

## Resultados (prueba real)

- **Antigravity:** `agy -p /usage` respondió sin gastar tokens. Kiti mostró «Antigravity 48 %» (la semanal de Claude y GPT, la más baja de sus grupos), igual que la salida de `agy`.
- **Claude:** con Claude Code 2.1.310 el JSON de la status line sí trae `rate_limits` (el issue #45133 no aplica a esta versión). Kiti mostró «Claude 34 %» semanal en cuanto se refrescó la status line de una sesión.
- **La status line del usuario no cambia:** la salida del script con y sin Kiti delante es idéntica (mismo md5 con el mismo JSON).
- **Interfaz:** el chip muestra solo lo semanal (`Claude 34 %`) para caber en la barra. El color es lo peor de 5 h y semana, y el detalle de cada ventana aparece al pasar el ratón.

## Límites conocidos

- La cuota de Claude solo se actualiza mientras alguna sesión refresca su status line. Sin sesiones abiertas, se ve la última conocida.
- La de Antigravity se consulta cada 5 minutos.
- Con la barra de la app nativa llena («+ Nuevo agente», «apagadas»), los contadores pasan a una segunda línea. Queda para el trabajo de UX en curso.

