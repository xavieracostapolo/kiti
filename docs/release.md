# Publicar un release de Kiti.app

El workflow [`release.yml`](../.github/workflows/release.yml) genera Kiti.app para macOS:

- Binario universal (Apple Silicon e Intel).
- Firmado con **Developer ID Application** y hardened runtime.
- **Notarizado** por Apple, con el ticket grapado en la app y en el DMG.

Un tag `vX.Y.Z` crea un **release en borrador** con el DMG, la app en `.zip` y `SHA256SUMS.txt`. Un PR que toque el workflow o la configuración del bundle solo compila sin firmar, para comprobar que el build funciona.

## Secrets del repositorio (una vez)

Hace falta una cuenta del Apple Developer Program. Configura los secrets en *Settings → Secrets and variables → Actions* o con `gh secret set`:

| Secret | Qué es |
|---|---|
| `APPLE_CERTIFICATE` | El `.p12` del certificado Developer ID Application con su clave privada, en base64 |
| `APPLE_CERTIFICATE_PASSWORD` | La contraseña con la que exportaste el `.p12` |
| `APPLE_SIGNING_IDENTITY` | El nombre exacto de la identidad, por ejemplo `Developer ID Application: Nombre (TEAMID1234)` |
| `APPLE_ID` | El Apple ID de la cuenta de desarrollador |
| `APPLE_PASSWORD` | Una contraseña de app de [appleid.apple.com](https://appleid.apple.com) (no la del Apple ID) |
| `APPLE_TEAM_ID` | El Team ID de 10 caracteres |

Pasos:

1. En **Acceso a Llaveros**, busca «Developer ID Application», despliégalo para incluir la clave privada y expórtalo como `.p12` con una contraseña. Si no tienes el certificado, créalo en [developer.apple.com → Certificates](https://developer.apple.com/account/resources/certificates/list).
2. Comprueba el nombre de la identidad: `security find-identity -v -p codesigning`.
3. Carga los secrets. Los comandos sin `--body` piden el valor sin mostrarlo:

   ```bash
   base64 -i DeveloperID.p12 | gh secret set APPLE_CERTIFICATE
   gh secret set APPLE_CERTIFICATE_PASSWORD
   gh secret set APPLE_SIGNING_IDENTITY --body "Developer ID Application: Nombre (TEAMID1234)"
   gh secret set APPLE_ID
   gh secret set APPLE_PASSWORD
   gh secret set APPLE_TEAM_ID --body "TEAMID1234"
   ```

4. Borra el `.p12` del disco cuando termines.

## Publicar una versión

1. Sube la versión en `src-tauri/tauri.conf.json` (y en los `Cargo.toml` y `package.json` si cambia). El workflow falla si el tag no coincide con esa versión.
2. Crea el tag desde `main` y súbelo:

   ```bash
   git tag -a v0.1.0 -m "Kiti 0.1.0"
   git push origin v0.1.0
   ```

3. Espera el workflow (unos 20 minutos; la notarización tarda lo que tarde Apple). Verifica la firma, los entitlements, Gatekeeper y el grapado antes de crear el release.
4. Revisa el borrador en *Releases*, edita las notas y publícalo.

## Permisos de macOS

- `src-tauri/entitlements.plist` da `com.apple.security.automation.apple-events`. Sin él, con hardened runtime, «Ir a su terminal» (ADR 0012) no puede controlar Ghostty ni Terminal.app.
- `src-tauri/Info.plist` añade `NSAppleEventsUsageDescription`, el texto que macOS muestra al pedir el permiso de Automatización.
- Las notificaciones (ADR 0010) las pide macOS la primera vez y no necesitan entitlement.
