# KryptoPass Desktop — guía de trabajo

App de escritorio en Rust + CXX-Qt + QML (Kirigami). QML presenta,
Rust decide y ejecuta. QML recibe metadatos observables y proyecciones
temporales del contenido necesario para el detalle o editor abierto; no
recibe claves privadas ni conserva secretos en modelos globales. Rust controla
el cifrado, la persistencia, la autorización de lectura y la invalidación.
Vaciar propiedades QML no garantiza borrar todas las copias de memoria Qt/JS.
El contrato completo vive en `docs/SECRET_DATA_CONTRACT.md`.

## Puertas: just manda, bacon vigila

```bash
just quick    # fmt + clippy: ciclo de edición, lo que corres mientras trabajas
just verify   # puerta completa en paralelo: clippy, lint QML, tests, packaging
just smoke    # arranque offscreen, falla ante errores QML
just ready    # verify + árbol limpio (gate pre-push)
bacon         # check continuo en fondo
bacon qmllint # o tecla q: lint QML en cada cambio (vigila crates/)
```

No corras `check` ni `verify` tras cada edición: usa `quick` mientras
iteras y `verify` una vez antes de commitear o dar el trabajo por listo.
`clippy` ya compila y regenera los `.qmltypes`, por eso `verify` no lleva
`check` aparte. Para un solo tipo de cambio corre solo su receta
(`just test-packaging`, `just lint`, `just test-items`).

`cargo lint` y `cargo format` existen como alias, pero `just` es la
referencia. Si agregas un chequeo nuevo, dale receta en el `justfile`
y job en `bacon.toml`, no un comando suelto en un doc.

## Reglas del bridge Rust/QML

- Cada `.qml` debe estar en `build.rs` (`QmlModule::qml_file`); si falta,
  el build muere con `Could not canonicalize path`.
- `lint` corre tras `check` para que los `.qmltypes` estén frescos.
  Con tipos viejos salen fantasmas (`bridgeStatus not found,
  did you mean bridge_status?`).
- Nombres QML explícitos con `cxx_name` (sin conversión automática
  snake→camel). Lo que QML lee debe existir en los tipos generados.
- Estilo fijo en código (`org.kde.desktop` salvo `QT_QUICK_CONTROLS_STYLE`);
  locale UTF-8 heredado del usuario en desarrollo; fallback `C.UTF-8` solo
  si el entorno no es UTF-8. CI y pruebas fijan el locale reproducible.

## Nombres e identidad

- Todo ID público usa DNS inverso `org.kryptopass.*`: módulo QML,
  `.desktop`, metainfo, futuros plugins y D-Bus. Sin excepciones.
- La identidad (nombre, versión, org) vive en `Cargo.toml` y se lee vía
  `app_info`; nada la duplica en `.rs`, `.qml` o instaladores.

## Reparto de agentes

- El **agente de implementación** toca producto. Antes de editar declara
  objetivo, archivos, plan y verificación. Sin aprobación no amplía alcance.
- Los **tests** los escribe un agente de tests con asignación explícita
  (un comportamiento, un archivo). Implementación no crea ni toca tests.
- Los **revisores** (skills de `comment-quality`) solo leen y reportan
  veredictos; nunca editan. Sin permiso de escritura no hay promesa:
  dáselo estructuralmente en el prompt del subagente.

## Skills locales

En `.agents/skills/`: `rust-comment-quality`, `qml-comment-quality`,
`commit-quality`. Se cargan bajo demanda; úsalas para revisiones y
mensajes de commit (Conventional Commits).

## Commits

Un cambio coherente por commit, mensaje en imperativo que describa el
resultado. `just ready` en verde antes de commitear.

## Desarrollo P2P

Antes de trabajar en P2P, leer `docs/p2p-implementation-plan.md` y
`docs/p2p-progress.md`. El primero conserva el plan completo del usuario;
el segundo registra las puertas y la evidencia de pruebas. Actualizar el
progreso al cerrar una fase. El primer hito termina con dos nodos Rust en
memoria, Noise XX, SAS y confianza persistente probados. No empezar WebRTC,
Android ni transferencia de claves privadas de producción antes de cumplir
las puertas correspondientes del plan.

Petición posterior del usuario (2026-10-18): adelantar la integración LAN entre
Desktop y Android tras confirmar manualmente pairing y reconexión por emulador.
Mantener pausados los agentes de tests y seguridad hasta que los reactive;
solo implementación y comprobaciones de compilación/apertura. Esta autorización
permite avanzar el laboratorio, no declarar aceptada su seguridad o sus suites.
Consultar `docs/lan-transport-v1.md`; no habilitar transferencia privada GPG.

Petición del usuario (2026-10-18): preparar primitivas observables de dispositivos
y mantener la sesión autenticada, sin implementar UI nueva. Las futuras pantallas
se construirán aparte con sus frameworks. Ver `docs/device-session-v1.md`.

Petición posterior del usuario (2026-10-18): el agente implementa UI/UX de
dispositivos en Desktop; el usuario implementa Android. Los ajustes comunes
y contratos de UI se registran también en docs Android. Ver
`docs/desktop-devices-ui-contract.md`. Las pausas de tests/seguridad continúan.

Petición vigente posterior (2026-10-18): reparar la integración funcional de
dispositivos en ambas apps, lector QR, ingreso de códigos y eliminación/reset
explícitos del perfil de vinculación. El usuario autoriza pruebas de estos
flujos al terminar; el agente de seguridad independiente continúa pausado.
El borrado de vinculación no elimina la bóveda ni claves OpenPGP.

Petición posterior vigente (2026-10-18): simplificar Android a Buscar en red y
Manual y QR. La identidad inicial se prepara automáticamente al confirmar
Emparejar una invitación válida si no existe identidad ni trust residual.
Esta autorización sustituye el paso manual Crear perfil, no permite regenerar
identidades existentes/corruptas ni reparar trust faltante automáticamente.

Petición posterior vigente (2026-10-18): implementar recuperación de conexión
tras corte Wi-Fi, cambio de dirección y reinicio de apps sin repetir QR/SAS
para identidades autorizadas. Escucha local de proceso y metadatos de routing
persistentes; nunca generar identidades/trust al reanudar. Desconexión humana
detiene recuperación; olvidar/reset eliminan su estado antes de borrar claves.
Pruebas funcionales autorizadas; seguridad independiente continúa pausada.
Ver docs/connection-recovery-v1.md.

Petición vigente posterior (2026-10-18): adelantar fase 6 para comunicación
por Internet con relay de señalización no confiable, Cloudflare Tunnel y
subdominio signal.kryptopass.org en proyecto separado. Mantener LAN, Noise/SAS,
pinning de identidades y limpieza existentes. STUN/TURN configurables; no
transferencia GPG ni sincronización. No atribuir seguridad de producción a
compilación o pruebas funcionales. Ver docs/internet-transport-v1.md.


Petición vigente posterior (2026-10-19): implementar respaldo TURN real mediante
Cloudflare Realtime y un emisor HTTPS independiente en el proyecto relay.
Credenciales temporales por petición firmada con identidad existente; token del
proveedor solo en configuración privada del servidor. La lista de uso TURN es
independiente del trust P2P y se provisiona por el operador, sin alta pública.
No regenerar identidades para habilitar TURN. El modo relay-only es una prueba
explícita y no admite fallback directo. Ver docs/turn-credentials-v1.md.
Pruebas funcionales autorizadas; revisión independiente sigue pausada.


Despliegue local TURN 2026-10-19: el lanzador de desarrollo instalado incluye
KP_TURN_BROKER_ORIGIN=https://signal.kryptopass.org. install-dev-desktop lo
regenera: conservar/reponer ese prefijo mientras se integra configuración de UI.
No incluir KP_TURN_RELAY_ONLY en el lanzador normal ni tokens del proveedor.


Petición vigente posterior (2026-10-21): integrar la bóveda local Android real
con PGPainless en Kotlin/JVM y reutilizar GnuPG Desktop. Pruebas funcionales con
claves sintéticas autorizadas, revisión independiente todavía pausada.
La recepción de la clave desde Desktop tras pairing, autorización local del
emisor y aceptación del teléfono será el flujo principal; importación manual
en pantalla dedicada como alternativa. El envío privado por red aún está
pendiente y mantiene su puerta de revisión/autorización por operación. Ver
docs/key-provisioning-flow.md y documentación local OpenPGP de Android.

Petición vigente posterior (2026-10-21): priorizar privacidad de red antes de
Sync/provisionamiento. Respetar rutas del sistema y la VPN del usuario; no añadir
interfaces físicas arbitrarias a ICE. La política «Exigir VPN» debe bloquear si
no se puede verificar su cumplimiento, con excepción LAN explícita y persistente.
Probar con perfiles aislados y conservar datos al actualizar las apps. El usuario
dejó el Samsung por USB para estas comprobaciones. No modificar VPN/firewall del
sistema ni prometer protección contra fugas por observar solo el par ICE final.
Ver docs/network-privacy-policy-v1.md. Pruebas funcionales autorizadas;
revisión independiente y transferencia de claves reales continúan pausadas.

Petición del usuario (2026-10-23): retirar de Git el material privado OpenPGP
sintético y continuar las pruebas con claves generadas localmente. No versionar
claves privadas OpenPGP de prueba en binarios, assets ni constantes Base64. Compartir la
fixture local entre builds debug mediante `KP_SYNTHETIC_FIXTURE_DIR`; release
no incorpora las claves de laboratorio. No regenerar silenciosamente una
fixture existente ni borrar ledgers para cambiarla. La revisión independiente
y la habilitación de claves privadas reales siguen pendientes.
