# Provisionamiento de la clave desde Desktop

Prioridad confirmada por el usuario el 2026-10-21: la preparación habitual de
Android será recibir la identidad OpenPGP seleccionada desde un Desktop ya
emparejado. Importar una copia protegida desde archivo es una alternativa con
pantalla dedicada (`Page.KEY_IMPORT`), no un requisito previo al pairing.

Estado: **flujo a implementar**, no habilitado. Ya están implementados el
canal Noise autenticado y el importador OpenPGP local, pero no el transporte
de claves entre ellos. Los mensajes KeyTransfer existentes solo reservan
formatos; por sí solos no conceden permiso ni demuestran transferencia.

## Flujo esperado

1. Completar pairing y comparación SAS, o reconectar comprobando exactamente
   las identidades autorizadas. Si falta trust, primero se vincula.
2. En Desktop, seleccionar el teléfono conectado y la identidad OpenPGP
   concreta. Mostrar destinatario, fingerprint completo y alcance del envío.
3. Solicitar autorización local de esa exportación con bóveda desbloqueada.
   Una confirmación anterior, la mera conexión o el QR no sirven de permiso.
4. Enviar una oferta autenticada y acotada sin material secreto. Android
   muestra quién ofrece la clave y su fingerprint, y pide aceptar o rechazar.
5. Solo tras la aceptación autenticada del teléfono, exportar mediante
   GnuPG el material protegido de esa única identidad y transmitirlo
   fragmentado **dentro del canal Noise existente**. Nada pasa por señalización.
6. Android comprueba tamaño, digest, formato, fingerprint, protección del
   bloque y capacidad de descifrado. Pide la contraseña OpenPGP localmente y
   biometría para guardarlo con el mismo almacén sellado de la importación.
7. Confirmar importación al Desktop únicamente tras persistencia correcta y
   prueba de descifrado de un desafío no sensible. Una descarga completa
   no se muestra como identidad importada si el usuario cancela la protección.

## Reglas que debe cumplir el siguiente hito

- Operación independiente de Sync: nunca transmitir automáticamente claves
  junto con credenciales ni al reconectar.
- Autorización de un solo uso vinculada a la sesión Noise/generación, los
  dos DeviceId, el transfer_id aleatorio y el fingerprint. Un cambio de sesión,
  desconexión, bloqueo, revocación o timeout la invalida; no se reanuda sola.
- Aceptación explícita previa a chunks y confirmación final distinta de esa
  aceptación. El esquema reservado necesita formalizar ambos mensajes y
  negociar la capacidad antes de habilitar el intercambio con otras versiones.
- Un único destinatario y transferencia activa, límites de memoria/tiempo,
  fragmentos ordenados y digest completo. El digest verifica integridad del
  objeto; la autenticación corresponde a Noise y al trust local.
- Sin claves, contraseñas, payloads Protobuf ni SDP en logs o reportes; sin
  temporales persistentes de exportación. El secreto nunca atraviesa QML/Compose.
- Importación create-only: una identidad existente o almacenamiento incoherente
  produce error; nunca reemplazo silencioso. La UI debe explicar la recuperación
  sin reiniciar trust ni borrar la bóveda existente.
- Pruebas con claves sintéticas y el mismo código de producto, incluyendo
  ausencia de aprobaciones, rechazo, repetición, sesión sustituida, cortes,
  expiración, fingerprint incorrecto, corrupción y fallo de persistencia.

Antes de habilitar claves privadas reales se mantiene la revisión de seguridad
independiente exigida por AGENTS.md y el plan. La autorización interactiva en
ambas apps sigue siendo necesaria incluso después de esa revisión. Esta
documentación no sustituye esa revisión ni convierte el laboratorio en una
transferencia de producción.
