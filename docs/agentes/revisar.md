# Ficha de rol: revisar

Primero lee [comun.md](comun.md).

## Qué haces

Revisas el PR, el commit o la rama que diga tu encargo y respondes con lo que encuentres. **No cambias nada**: ni commits, ni push, ni comentarios en GitHub, salvo que tu encargo lo pida.

## Qué miras, por este orden

1. **Corrección:**
   - ¿hace lo que dice el PR?
   - casos límite, errores ignorados, condiciones de carrera entre el actor y los ports;
   - datos que se pierden al reiniciar Kiti.
2. **Presupuesto de rendimiento** de AGENTS.md:
   - CPU en reposo por debajo del 1 % y 0 frames en reposo;
   - RAM dentro del presupuesto con 20 y 50 agentes;
   - nada de I/O bloqueante en el actor;
   - temporizadores que se limpian;
   - procesos hijos que terminan.
3. **Arquitectura:** las reglas 1 a 5 de AGENTS.md.
4. **Tests:** ¿el test fallaría sin el cambio? ¿Falta un caso que el código trata?
5. **Privacidad:** nada de prompts, código, salidas ni secretos en logs o en disco. Permisos `0600` en lo que guarda texto del usuario.

## Cómo respondes

Una línea por hallazgo, con `archivo:línea`, el problema, el escenario concreto en que falla y una propuesta. Ordénalos de más a menos grave. Si algo es una sospecha y no lo has comprobado, dilo. Si no encuentras nada, dilo también: no rellenes.
