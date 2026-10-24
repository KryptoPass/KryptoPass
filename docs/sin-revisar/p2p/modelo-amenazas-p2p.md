# Modelo de amenazas del núcleo P2P

Referencia normativa del proyecto: [plan v1.0](plan-implementacion-p2p.md).
El estado de implementación y las pruebas están en [progreso](progreso-p2p.md).
Este documento define qué se debe auditar; no declara terminadas las medidas.

## Fronteras de confianza

El transporte solo entrega bytes. El parsing del protocolo limita recursos y
valida estructura, versión y secuencia. Noise protege el canal de aplicación;
las firmas vinculan las identidades de dispositivo. En el primer pairing,
la aprobación visual local y la confirmación remota autenticada establecen
confianza. SQLite debe confirmar la escritura antes de exponer `Trusted`.

OpenPGP protege el contenido de las credenciales. La identidad Ed25519 y
X25519 de cada dispositivo es independiente de la identidad OpenPGP compartida.
El primer hito no abre bóvedas ni importa, exporta o transfiere claves GPG.

## Adversarios y comprobaciones

| Adversario / fallo | Comportamiento exigido |
|---|---|
| Equipo desconocido en LAN | Puede intentar conectar; no adquiere confianza por ser descubierto |
| Anuncios mDNS falsos | Solo cambian localización; no autorizan identidades |
| Señalización controlada | No autoriza dispositivos; Noise y la comparación humana del SAS permiten detectar sustituciones |
| Intermediario activo | Identidades y transcript ligados; verificar pruebas y comparar SAS antes de autorizar, con el límite probabilístico indicado abajo |
| Equipo conocido con claves sustituidas | Rechazo; no reemplazar las claves guardadas automáticamente |
| Dispositivo perdido o robado | Revocar autorización P2P; prever rotación OpenPGP y exclusión |
| SQLite corrupto / escritura fallida | Fallar sin promover un dispositivo a confiable |
| Repetición o desorden de paquetes | Rechazo por sesión/secuencia y por autenticación Noise |
| Paquete alterado | Rechazo antes de aceptar datos o confirmar confianza |
| Parsing o fragmentación maliciosa | Tamaños, cantidades y tiempos acotados, sin reservas basadas en cifras no validadas |
| Reinicio antes de aprobación | No existe registro de confianza parcial que se restaure como autorizado |
| Invitación expirada o consumida | No permite iniciar otra sesión de pairing |
| Cancelación o timeout antes de commit | Termina el proceso y no escribe confianza |

El catálogo visual es el de [Matrix SAS emoji](https://spec.matrix.org/v1.16/client-server-api/#sas-method-emoji).
KryptoPass define su propia composición de Noise, firmas y HKDF; no es una
implementación interoperable del protocolo Matrix.

La representación SAS tiene 42 bits: contextos distintos pueden producir los
mismos siete símbolos. Para salidas independientes uniformes la probabilidad
de coincidencia es 2^-42 por comparación; esto no establece por sí solo una
cota frente a un atacante adaptativo que elija múltiples transcripts. Esta
composición requiere revisión independiente antes de uso sensible en producción.

El límite de tiempo se comprueba inmediatamente antes de empezar la escritura
local de confianza. Una transacción SQLite síncrona ya iniciada no se interrumpe:
el resultado debe reflejar si quedó persistida, incluso si termina después del
plazo. Cada equipo confirma su propia transacción; no existe atomicidad entre
ambas bases. El contrato preciso está en [emparejamiento-autenticado-v1.md](../protocolo/emparejamiento-autenticado-v1.md).

## Límites de las garantías

Una aplicación o un sistema operativo comprometidos pueden mentir en la UI,
robar claves desbloqueadas o alterar el registro de confianza. No se promete
protección absoluta frente a dispositivos ya comprometidos. Los errores de
persistencia y el almacenamiento malformado deben rechazarse, pero detectar
un rollback malicioso de todo el perfil requiere un diseño adicional.

Compartir una clave privada OpenPGP implica que el compromiso de una copia
puede comprometer los secretos cifrados con esa identidad. La revocación P2P
no elimina claves ya copiadas ni impide descifrar ciphertext histórico.
Se necesitan políticas futuras de rotación de claves, exclusión de equipos,
respaldos cifrados y recuperación de una pérdida de identidad local.

El almacén seguro del sistema operativo debe fallar con un error explícito
si no está disponible. No se debe convertir ese error en escritura de claves
privadas sin protección en SQLite, JSON, archivos temporales o logs.
Las pruebas usarán exclusivamente identidades sintéticas y perfiles aislados.

La composición Noise + SAS necesita revisión de seguridad independiente
antes de habilitar transferencia de claves privadas en producción. La
auditoría del coordinador y las pruebas de este hito no sustituyen esa revisión.
