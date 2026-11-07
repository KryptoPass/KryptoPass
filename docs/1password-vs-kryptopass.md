# 1Password vs KryptoPass: tabla de features

**Leyenda:** ✅ hecho · 🟡 parcial · ❌ no existe · ⛔ fuera de alcance.
Estado según el código de Desktop al 2026-10-26. Android es una extensión de Desktop y hereda su estado.
Sync, Internet y transferencia de claves están en laboratorio (ver README).

| Área | Feature | 1Password | KryptoPass |
| --- | --- | --- | --- |
| Apps | Apps de escritorio | macOS, Windows y Linux | 🟡 Linux (KDE Plasma/Qt 6); sin macOS ni Windows |
| Apps | Apps móviles | iOS/iPadOS, watchOS y Android | 🟡 Android como extensión de Desktop (laboratorio); sin iOS |
| Apps | Extensión de navegador | Chrome, Firefox, Edge, Brave y Safari | 🟡 Transporte cifrado y autorización por petición (`kp-browser`); la extensión parte de `keepassxc-browser` |
| Apps | App web | 1Password.com | ❌ No (local-first por diseño) |
| Cuenta | Sincronización entre dispositivos | Sí | 🟡 Sync P2P con ledger, en laboratorio; sin servidor de bóveda |
| Cuenta | Soporte para varias cuentas | Sí | ❌ Una bóveda/identidad por instalación |
| Productividad | Quick Access / búsqueda global | Sí, en macOS, Windows y Linux | 🟡 Búsqueda privada en memoria dentro de la app; sin acceso rápido global |
| Organización | Bóvedas privadas | Sí | 🟡 Una bóveda; carpetas para organizar |
| Organización | Bóvedas compartidas | Sí | 🟡 Destinatarios OpenPGP por item (`RecipientSet`) |
| Organización | Colecciones de bóvedas | Sí | ❌ No |
| Organización | Etiquetas, favoritos y búsqueda | Sí | 🟡 Favoritos, carpetas anidadas, papelera y búsqueda; sin etiquetas |
| Items | Logins y contraseñas | Sí, con autoguardado | 🟡 Logins con varias URL, notas y TOTP; sin autoguardado |
| Items | Autofill en webs y apps | Sí | 🟡 Solo web vía extensión del navegador; sin apps |
| Items | Generador de contraseñas | Sí | ✅ Aleatorio con muestreo sin sesgo; clases, longitud y exclusiones |
| Items | Generador de nombres de usuario | Sí | ❌ No |
| Items | Passkeys | Crear, guardar, usar y compartir | ❌ No (el importador solo conserva passkeys de Proton como referencias) |
| Items | Códigos 2FA/TOTP | Integrados con autofill | ✅ Tipo propio `Totp` y campo TOTP en logins; QR local |
| Items | Tarjetas de crédito y débito | Sí | ✅ Tipo `Card` |
| Items | Direcciones e identidades | Sí | ✅ Tipo `Identity` |
| Items | Notas seguras | Sí, con Markdown | ✅ Tipo `SecureNote` con editor Markdown |
| Items | Documentos y archivos adjuntos | 1 GB Individual; 1 GB por persona en Families | ❌ No (solo iconos PNG como assets) |
| Items | Tipos de item especializados | Banco, cripto, base de datos, pasaporte, médico, servidores, etc. | 🟡 Login, API token, TOTP, SSH, nota, tarjeta, identidad, Wi-Fi, licencia y personalizado |
| Items | Campos personalizados | Sí | 🟡 Tipo `Custom`; los logins no tienen campos extra |
| Items | Historial de contraseñas y versiones | Sí, con restauración | ✅ Hasta 20 versiones cifradas y eventos de uso; con restauración |
| Items | Códigos QR para Wi-Fi | Sí | ✅ Genera `WIFI:T:...` desde el item Wi-Fi |
| Compartir | Enlaces caducables para compartir items | Sí, con o sin cuenta de 1Password | ❌ No |
| Compartir | Guests | Hasta 5, con acceso a una sola bóveda | ❌ No |
| Familia | Plan familiar | Hasta 5 miembros | ⛔ Fuera de alcance (sin cuentas ni servidor) |
| Familia | Family Organizer | Gestión de miembros, bóvedas, permisos y facturación | ⛔ Fuera de alcance |
| Familia | Recuperación por organizador | Sí | ⛔ Fuera de alcance |
| Recuperación | Emergency Kit | PDF con datos de acceso y Secret Key | 🟡 Copia `.kpb` cifrada y llavero GnuPG; sin PDF |
| Recuperación | Recovery codes | Individual y Families | ❌ No |
| Seguridad | Cifrado extremo a extremo | AES-GCM-256 | ✅ OpenPGP; el contenido se cifra antes de guardarse (nombres y carpetas quedan sin cifrar) |
| Seguridad | Secret Key | 128 bits, combinada con la contraseña | 🟡 Clave OpenPGP en el llavero GnuPG, sin Secret Key propia |
| Seguridad | Derivación y autenticación | PBKDF2 y SRP | 🟡 La delega GnuPG; no hay cuenta en servidor ni SRP |
| Seguridad | 2FA de la cuenta | App autenticadora o llave FIDO U2F | ⛔ No aplica (no hay cuenta) |
| Seguridad | Desbloqueo biométrico | Sí, con biometría o autenticación del sistema | ❌ No en Desktop |
| Seguridad | Auto-bloqueo | Configurable | ✅ Al bloquear la sesión, suspender o por inactividad configurable |
| Seguridad | Limpieza del portapapeles | A los 90 segundos por defecto | ✅ Portapapeles sensible con borrado automático configurable |
| Seguridad | Protección antiphishing | Solo autocompleta en el sitio guardado | 🟡 Autorización por petición y comprobación de origen en el navegador |
| Privacidad | Travel Mode | Oculta bóvedas no marcadas como seguras | ❌ No |
| Watchtower | Brechas y webs comprometidas | Sí | ❌ No |
| Watchtower | Contraseñas débiles o reutilizadas | Sí | ❌ No |
| Watchtower | Passkeys y 2FA disponibles | Sí | ❌ No |
| Watchtower | Items duplicados, caducados o con HTTP | Sí | ❌ No |
| Watchtower | Secretos de desarrollador en disco | Claves SSH y archivos `.env` | ❌ No |
| Migración | Importación desde otros gestores | Chrome, Firefox, Edge, Safari, Bitwarden, LastPass, etc. | 🟡 Solo Proton Pass |
| Migración | Exportación de datos | Sí | 🟡 Solo respaldo cifrado `.kpb`; sin CSV/JSON |
| IA | 1Password for Agents | Agentes piden login; la persona aprueba; el secreto no se expone | ❌ No |
| Planes | Prueba gratuita | 14 días | ⛔ No aplica (GPL-3.0, gratuito) |
| Desarrollo | CLI | Desbloqueo biométrico, secret references, gestión de items y usuarios | ❌ No |
| Desarrollo | Inyección de secretos | `op run` y `op inject` | ❌ No |
| Desarrollo | Shell plugins | AWS, GitHub, GitLab, Terraform, OpenAI CLI, Claude Code, etc. | ❌ No |
| Desarrollo | Environments | Variables por proyecto, importación de `.env` y lectura por CLI/SDK | ❌ No |
| Desarrollo | `.env` montado localmente | macOS y Linux, sin escribir secretos en disco | ❌ No |
| Desarrollo | Agent hooks | Validación para Cursor, Claude Code, Copilot y Windsurf | ❌ No |
| Desarrollo | MCP Server de Environments | Gestión de entornos con aprobación humana | ❌ No |
| Desarrollo | Sync con AWS Secrets Manager | Beta | ❌ No |
| Desarrollo | SSH y Git | Claves SSH, SSH Agent, firma de commits y autofill de claves públicas | ❌ No |
| Desarrollo | SDKs | Go, JavaScript/TypeScript y Python | ❌ No |
| Desarrollo | SDK: gestión completa | Items, bóvedas, grupos y permisos | ❌ No |
| Desarrollo | Service Accounts | Tokens con alcance limitado, hasta 100 por cuenta | ❌ No |
| Desarrollo | Connect Server | REST API self-hosted con caché local | ❌ No |
| Desarrollo | CI/CD | GitHub Actions, CircleCI y Jenkins | ❌ No |
| Desarrollo | Kubernetes | Integraciones con Service Account o Connect | ❌ No |
| Desarrollo | Terraform y Pulumi | Providers oficiales | ❌ No |
| Desarrollo | Credential Broker | Public preview con identidad de workload por OIDC | ❌ No |
| Desarrollo | VS Code | Detección de secretos, referencias, generación y previsualización | ❌ No |
| Desarrollo | Developer Watchtower | Escaneo de `~/.ssh` y `.env` en disco | ❌ No |
| Desarrollo | APIs de administración | Events, Users, SaaS Manager y Partnership | ❌ No |
| Desarrollo | Integración web | Save in 1Password y guías de compatibilidad | ❌ No |
| Desarrollo | Documentación para IA | `llms.txt` y páginas en Markdown | ❌ No |
| Desarrollo | Open source y formación | 1Password for Open Source, Academy y comunidad | ❌ No |
