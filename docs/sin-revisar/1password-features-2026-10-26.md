# 1Password: funciones para usuarios y desarrolladores

Fecha: 2026-10-26. Estado: investigación de producto basada en documentación
oficial. Este documento no describe KryptoPass ni declara una integración;
ordena las capacidades publicadas por 1Password para uso personal/familiar y
para desarrollo.

1Password cubre dos capas que comparten la misma base cifrada:

- Un gestor de contraseñas, passkeys, datos personales y documentos para
  personas y familias.
- Una plataforma de secretos para terminal, IDE, CI/CD, contenedores y
  aplicaciones.

Las herramientas para desarrolladores están incluidas en los planes
Individual y Families, además de los planes de equipo y empresa. Las funciones
marcadas como preview o beta pueden cambiar.

## Mapa general

| Necesidad | Capacidades principales |
| --- | --- |
| Gestión diaria | Apps multiplataforma, autofill, passkeys, códigos 2FA, tarjetas, direcciones, notas y archivos |
| Familia y sharing | Bóvedas privadas y compartidas, guests, enlaces caducables, recuperación por organizador |
| Seguridad personal | Cifrado extremo a extremo, Secret Key, 2FA, biometría, auto-bloqueo, Travel Mode, Watchtower |
| Desarrollo local | CLI, secret references, `op run`, `op inject`, shell plugins, Environments, VS Code, SSH Agent |
| Automatización | SDKs, Service Accounts, Connect Server, CI/CD, Kubernetes, Terraform, Pulumi |
| IA y agentes | 1Password for Agents, MCP Server de Environments, agent hooks, OpenShell |
| Administración | Events API, Users API, SaaS Manager API, Partnership API, reportes de uso |

## Parte I. Usuarios normales

### 1. Apps y acceso

- Apps para macOS, Windows, Linux, iOS/iPadOS, watchOS y Android.
- Extensión para Chrome, Firefox, Edge, Brave y Safari, además de la app web.
- Sincronización automática de la bóveda entre dispositivos y soporte para
  varias cuentas.
- **Quick Access** en macOS, Windows y Linux para buscar items y copiar o
  autocompletar credenciales sin cambiar de aplicación.
- Desbloqueo con biometría, autenticación del sistema o contraseña de la
  cuenta; auto-bloqueo configurable.
- Organización mediante bóvedas, **colecciones** (grupos personalizados de
  bóvedas), etiquetas, favoritos y búsqueda global.

### 2. Guardar y autocompletar

- **Logins y contraseñas** con autoguardado y autofill en sitios web y apps
  compatibles.
- **Passkeys**: creación, guardado, inicio de sesión, organización y uso en
  sitios y apps compatibles; Watchtower avisa dónde se puede migrar.
- **Códigos 2FA/TOTP** integrados en el item, con autofill del código.
- **Tarjetas de crédito y débito**, direcciones e identidades para rellenar
  formularios.
- **Notas seguras** con formato Markdown.
- **Documentos y archivos** adjuntos: 1 GB en Individual y 1 GB por persona
  en Families.
- Generador de contraseñas y generador de nombres de usuario.
- Campos personalizados, historial de contraseñas, versiones anteriores de un
  item y restauración.
- Códigos QR para compartir redes Wi-Fi.
- Tipos de item disponibles: Login, Password, Secure Note, Credit Card,
  Identity, Document, API Credential, Bank Account, Crypto Wallet, Database,
  Driver License, Email Account, Medical Record, Membership, Outdoor License,
  Passport, Reward Program, SSH Key, Server, Social Security Number, Software
  License y Wireless Router.

### 3. Compartir y familia

- **1Password Families** admite hasta cinco miembros, cada uno con su bóveda
  privada.
- Bóvedas compartidas ilimitadas con permisos para ver, editar o gestionar.
- **Guests**: hasta cinco invitados con acceso temporal a una sola bóveda;
  se pueden promover a miembros de la familia.
- **Compartir un item con cualquiera** mediante un enlace caducable,
  restringido por email o disponible para quien tenga el enlace, aunque no use
  1Password.
- Historial de enlaces compartidos y opción de revocarlos.
- Quien recibe un item puede guardar una copia en su propia cuenta.
- Los passkeys también se pueden compartir mediante bóvedas o enlaces.
- **Family Organizer**: invitar miembros y guests, crear y compartir bóvedas,
  gestionar permisos, facturación y recuperar cuentas.

### 4. Seguridad

- Cifrado **extremo a extremo** con AES-GCM-256.
- **Secret Key** de 128 bits combinada con la contraseña de la cuenta; PBKDF2
  para derivación de claves y SRP para autenticación sin enviar la contraseña.
- **2FA para la cuenta de 1Password** con app autenticadora o llave de
  seguridad FIDO U2F compatible (YubiKey, Titan).
- Protección antiphishing: solo autocompleta en el sitio donde se guardó la
  credencial.
- Limpieza automática del portapapeles después de 90 segundos por defecto.
- Validación de firma del navegador antes de autocompletar.
- **Travel Mode**: elimina de las apps todas las bóvedas salvo las marcadas
  como «Safe for Travel».
- Telemetría opcional y herramientas de exportación de datos.

### 5. Watchtower

- Alertas de filtraciones y sitios comprometidos.
- Detección de contraseñas débiles, reutilizadas y vulneradas.
- Avisos de sitios que ya admiten **passkeys** o **2FA**.
- Detección de sitios con HTTP sin cifrar, items duplicados e items guardados
  en otra cuenta.
- Avisos de caducidad para tarjetas, pasaportes, licencias y credenciales API.
- En escritorio, detección de secretos de desarrollador en disco, como claves
  SSH sin cifrar y archivos `.env` con valores en claro.

### 6. Recuperación

- **Emergency Kit** en PDF con dirección de inicio de sesión, email, Secret
  Key y espacio para anotar la contraseña.
- **Recovery codes** reutilizables para cuentas Individual y Families.
- Recuperación por **Family Organizer**: genera una nueva Secret Key, obliga a
  crear una nueva contraseña y restablece el 2FA.
- Después de una recuperación hay que volver a iniciar sesión en todos los
  dispositivos.

### 7. Travel Mode

- Disponible en todas las suscripciones.
- Antes de viajar se activa desde la cuenta y se marcan las bóvedas seguras.
- Las bóvedas no marcadas desaparecen de las apps, de la extensión y de
  1Password.com mientras el modo está activo.
- Al desactivarlo, las bóvedas vuelven a estar disponibles tras sincronizar.

### 8. IA y agentes

- **1Password for Agents** permite que agentes compatibles pidan un login y
  que la persona apruebe o deniegue el acceso en su dispositivo.
- 1Password rellena la contraseña o el código 2FA; el agente no recibe el
  secreto ni obtiene acceso a toda la bóveda.
- Integraciones anunciadas con agentes como Muse (Meta), Claude, ChatGPT Work,
  dots y Grok Bot; el despliegue es gradual y puede requerir lista de espera.
- Incluido en los planes Individual y Families.

### 9. Migración y planes

- Importación desde Chrome, Firefox, Edge, Brave, Safari, Bitwarden, Dashlane,
  KeePass, KeePassXC, Keeper, LastPass, RoboForm y archivos CSV.
- Importación directa mediante **Credential Exchange** en iOS 26 o posterior
  y Android 14 o posterior.
- Importación de exportaciones `.1pux` de 1Password.
- Prueba gratuita de 14 días; no hay plan gratuito permanente.
- Los planes personales incluyen también las herramientas para
  desarrolladores.

## Parte II. Desarrolladores

### 1. CLI (`op`)

- Desbloqueo biométrico mediante la integración con la app de escritorio.
- **Secret references** con sintaxis `op://vault/item/field` para no guardar
  secretos en claro.
- `op run`: inyecta variables de entorno al ejecutar un proceso.
- `op inject`: renderiza plantillas y archivos de configuración con secretos.
- `op read`: obtiene un secreto puntual.
- CRUD de items, bóvedas, grupos y permisos; provisión de usuarios.
- **Shell plugins** para autenticar CLIs con huella, por ejemplo AWS, GitHub,
  GitLab, Terraform, OpenAI CLI y Claude Code.
- Integración con Service Accounts, Connect Server, Environments y CI/CD.

### 2. Environments

- Variables de entorno por proyecto o etapa, separadas de los items normales
  de la bóveda.
- Importación desde archivos `.env` existentes.
- **`.env` montado localmente** sin escribir los secretos en disco; disponible
  en macOS y Linux.
- Lectura programática mediante CLI o SDKs.
- **Agent hooks** que validan el `.env` antes de que agentes como Cursor,
  Claude Code, GitHub Copilot o Windsurf ejecuten comandos.
- **MCP Server de Environments** para crear y gestionar entornos con
  aprobación por acción, sin devolver secretos al agente.
- Sincronización con **AWS Secrets Manager** (beta).
- Uso compartido con permisos por entorno.
- La función no está disponible en las apps de iOS y Android.

### 3. SSH y Git

- Generación, importación y almacenamiento de claves SSH.
- **SSH Agent** con aprobación biométrica por solicitud; la clave privada no
  sale de 1Password.
- **Firma de commits Git con SSH**.
- Autofill de claves públicas en GitHub, GitLab y otras plataformas.
- Configuración del agente por host o bóveda.
- Registro de actividad reciente del SSH Agent.

### 4. SDKs

- SDKs open source para Go, JavaScript/TypeScript y Python.
- Carga de secretos mediante referencias y lectura de variables de
  Environments.
- Gestión completa de items: crear, leer, actualizar, borrar, listar y
  compartir.
- Gestión de bóvedas, grupos y permisos.
- Autenticación local con aprobación en la app de escritorio o con token de
  Service Account.
- Integrables en aplicaciones de producción, scripts y agentes de IA.
- Los SDKs están en versión 0; pueden introducir cambios incompatibles entre
  releases de esa versión.

### 5. Automatización y despliegue

- **Service Accounts**: tokens con alcance limitado a bóvedas y Environments,
  hasta 100 por cuenta, con reportes de uso.
- **Connect Server**: REST API self-hosted, caché local, sin cuotas por
  reconsultas y control sobre la infraestructura. Los límites internos se
  aplican en la primera obtención de secretos.
- Integraciones CI/CD para **GitHub Actions, CircleCI y Jenkins**.
- Integraciones con **Kubernetes**, mediante Service Account o Connect.
- **Terraform provider** y **Pulumi provider**.
- **Credential Broker** (public preview): acceso por identidad de workload
  mediante OIDC, con integración para GitHub Actions y workflows
  personalizados.
- Sincronización de Environments con AWS Secrets Manager (beta).

### 6. IA y agentes

- Secretos para `mcp.json` mediante `op run --environment`, sin tokens en
  claro.
- **MCP Server** para gestionar Environments con aprobación humana.
- Plugins para **Claude Code** y **Cursor**.
- **NVIDIA OpenShell** (beta): el agente trabaja con placeholders y el gateway
  inyecta los secretos fuera del sandbox.
- Shell plugins biométricos para CLIs de IA.
- La extensión de VS Code detecta secretos en claro antes de que entren al
  contexto de un modelo.

### 7. IDE y productividad

- **1Password for VS Code**: detectar secretos, guardarlos, obtenerlos,
  generar contraseñas, previsualizar referencias y abrir items.
- **Developer Watchtower**: escanea `~/.ssh` y archivos `.env` en disco,
  alerta sobre claves inseguras o sin cifrar y permite importarlas o
  eliminarlas.
- 1Password for Open Source, 1Password Academy y proyectos de comunidad.
- Documentación legible por IA mediante `llms.txt` y páginas en Markdown.

### 8. APIs de gestión y auditoría

- **Events API**: eventos de auditoría hacia SIEM y plataformas de analítica.
- **Users API**: suspender, reactivar o consultar usuarios durante una
  respuesta a incidentes.
- **SaaS Manager API**: gestión programática de aplicaciones, personas,
  equipos, contratos, dispositivos y workflows.
- **Partnership API**: aprovisionamiento de cuentas y facturación para
  partners.
- Políticas de administración para Environments, escaneo de disco y
  herramientas de desarrollador.
- **Save in 1Password** para integraciones web y guías de compatibilidad de
  sitios.

## Funciones compartidas y límites

- Las herramientas de desarrollo (CLI, SSH, SDKs y parte de Secrets
  Automation) están incluidas en los planes personales, no solo en los de
  empresa.
- Passkeys y códigos 2FA son funciones de usuario final que también resultan
  útiles en flujos de desarrollo.
- Watchtower cubre tanto la seguridad personal como la detección de secretos
  de desarrollador en disco.
- 1Password for Agents está incluido en Individual y Families, pero su
  disponibilidad depende del agente, la región, el dispositivo y el método de
  inicio de sesión.
- El desbloqueo de 1Password con passkey está en beta pública para cuentas
  Individual nuevas; para cuentas existentes figura como próximamente.
- Los recovery codes no están disponibles en cuentas de equipo o empresa; en
  esos casos la recuperación la realiza un administrador.
- La sincronización con AWS Secrets Manager y NVIDIA OpenShell están en beta;
  Credential Broker está en public preview.
- Las funciones locales de Environments (`.env` montado y agent hooks) tienen
  disponibilidad limitada por plataforma.

## Fuentes

Consultadas el 2026-10-26:

- https://1password.com/features/
- https://1password.com/product/password-manager
- https://1password.com/product/passkeys
- https://1password.com/personal-ai-agent-integrations
- https://support.1password.com/
- https://support.1password.com/1password-security/
- https://support.1password.com/item-categories/
- https://support.1password.com/share-items/
- https://support.1password.com/create-share-vaults/
- https://support.1password.com/guests/
- https://support.1password.com/family-organizer/
- https://support.1password.com/recovery/
- https://support.1password.com/recovery-codes/
- https://support.1password.com/emergency-kit/
- https://support.1password.com/travel-mode/
- https://support.1password.com/watchtower/
- https://support.1password.com/one-time-passwords/
- https://support.1password.com/two-factor-authentication/
- https://support.1password.com/security-key/
- https://support.1password.com/quick-access/
- https://support.1password.com/collections/
- https://support.1password.com/files/
- https://support.1password.com/import/
- https://developer.1password.com/
- https://www.1password.dev/cli.md
- https://www.1password.dev/environments.md
- https://www.1password.dev/ssh.md
- https://www.1password.dev/sdks.md
- https://www.1password.dev/service-accounts.md
- https://www.1password.dev/connect.md
- https://www.1password.dev/ci-cd.md
- https://www.1password.dev/watchtower.md
- https://www.1password.dev/vscode.md
- https://www.1password.dev/brokered-access.md
- https://www.1password.dev/get-started/secure-ai-access.md
