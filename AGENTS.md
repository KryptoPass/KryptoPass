# KryptoPass Desktop — guía de trabajo

App de escritorio en Rust + CXX-Qt + QML (Kirigami). QML presenta,
Rust decide y ejecuta. QML recibe metadatos observables y proyecciones
temporales del contenido necesario para el detalle o editor abierto; no
recibe claves privadas ni conserva secretos en modelos globales. Rust controla
el cifrado, la persistencia, la autorización de lectura y la invalidación.
Vaciar propiedades QML no garantiza borrar todas las copias de memoria Qt/JS.

## Puertas: just manda, bacon vigila

```bash
just quick    # fmt + clippy: lo corre el usuario; el agente usa `cargo check` al editar
just verify   # puerta completa: clippy, lint QML, tests, packaging (la corre el usuario)
just smoke    # arranque offscreen, falla ante errores QML (lo corre el usuario)
just ready    # verify + árbol limpio (gate pre-push)
bacon         # check continuo en fondo
bacon qmllint # o tecla q: lint QML en cada cambio (vigila crates/)
```

Mientras edita, al agente le basta `cargo check`. Al terminar corre una sola
vez la puerta acotada a lo que tocó: `cargo fmt --all --check`, `cargo clippy
-p <crates> --all-targets -- -D warnings` y los tests de esos crates o su
receta (`just test-search`, `just test-sync`, `just lint` si cambió QML…).
`verify`, `quick`, `smoke` y `ready` los corre el usuario; el agente no los
lanza por su cuenta. Si solo falla el formato, basta con formatear.
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
resultado. Puerta acotada en verde antes de commitear; `just verify` y
`just ready` los corre el usuario.

## Desarrollo P2P

El primer hito termina con dos nodos Rust en memoria, Noise XX, SAS y confianza
persistente probados. No empezar WebRTC, Android ni transferencia de claves
privadas de producción antes de cumplir las puertas correspondientes. Al cerrar
una fase, registrar las puertas y la evidencia de pruebas.

Peticiones vigentes del usuario (2026-10-18 a 2026-10-23), como reglas:

- La integración LAN Desktop ↔ Android, el pairing y la reconexión son
  laboratorio: no declarar aceptada su seguridad ni sus suites. Los agentes de
  tests y seguridad siguen pausados hasta que el usuario los reactive; solo
  implementación y comprobaciones de compilación/apertura. No habilitar
  transferencia privada GPG.
- El agente implementa la UI/UX de dispositivos en Desktop; el usuario
  implementa Android. Los ajustes comunes y contratos de UI se registran
  también en la documentación Android.
- El borrado de vinculación no elimina la bóveda ni claves OpenPGP. La
  identidad inicial solo se prepara automáticamente al emparejar una invitación
  válida si no existe identidad ni trust residual; nunca se regeneran
  identidades existentes o corruptas ni se repara trust faltante.
- Recuperación de conexión sin repetir QR/SAS para identidades autorizadas:
  escucha local de proceso y metadatos de routing persistentes; nunca generar
  identidades/trust al reanudar. La desconexión humana detiene la recuperación;
  olvidar/reset eliminan su estado antes de borrar claves.
- Internet: relay de señalización no confiable (Cloudflare Tunnel,
  signal.kryptopass.org, proyecto separado), STUN/TURN configurables, sin
  transferencia GPG ni sincronización. Mantener LAN, Noise/SAS, pinning de
  identidades y limpieza. No atribuir seguridad de producción a compilación o
  pruebas funcionales.
- TURN real mediante Cloudflare Realtime y un emisor HTTPS independiente:
  credenciales temporales por petición firmada con identidad existente; el
  token del proveedor solo vive en configuración privada del servidor. La lista
  de uso TURN es independiente del trust P2P y la provisiona el operador, sin
  alta pública. No regenerar identidades para habilitar TURN. El modo
  relay-only es una prueba explícita y no admite fallback directo.
- `just run` exporta `KP_TURN_BROKER_ORIGIN=https://signal.kryptopass.org`
  salvo que ya esté definido, y `just install` lo fija en el lanzador con la
  opción CMake `KRYPTOPASS_TURN_BROKER_ORIGIN`. No incluir `KP_TURN_RELAY_ONLY`
  ni tokens del proveedor.
- Bóveda Android real con PGPainless (Kotlin/JVM) reutilizando GnuPG Desktop;
  pruebas con claves sintéticas. La recepción de la clave desde Desktop tras
  pairing, autorización local del emisor y aceptación del teléfono es el flujo
  principal; la importación manual en pantalla dedicada es la alternativa. El
  envío privado por red sigue pendiente y mantiene su puerta de
  revisión/autorización por operación.
- Privacidad de red antes de Sync/provisionamiento: respetar rutas del sistema
  y la VPN del usuario; no añadir interfaces físicas arbitrarias a ICE. «Exigir
  VPN» debe bloquear si no se puede verificar su cumplimiento, con excepción
  LAN explícita y persistente. Probar con perfiles aislados y conservar datos
  al actualizar las apps. No modificar VPN/firewall del sistema ni prometer
  protección contra fugas por observar solo el par ICE final.
- No versionar claves privadas OpenPGP de prueba en binarios, assets ni
  constantes Base64: usar claves generadas localmente y compartir la fixture
  entre builds debug con `KP_SYNTHETIC_FIXTURE_DIR`. Release no incorpora las
  claves de laboratorio. No regenerar silenciosamente una fixture existente ni
  borrar ledgers para cambiarla. La revisión independiente y la habilitación de
  claves privadas reales siguen pendientes.
