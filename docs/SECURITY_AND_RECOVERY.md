# Seguridad y recuperación

KryptoPass usa GPGME/GnuPG para OpenPGP, SQLite para la bóveda local,
Qt para el portapapeles y Akonadi/KContacts para la agenda. No mantiene
otro llavero ni otra base de contactos.

## Destinatarios y bloqueo

Cada secreto conserva su conjunto explícito de huellas. Crear un elemento
requiere elegir al menos una clave; editar conserva la selección anterior.
Moverlo de carpeta no cambia los destinatarios. Cambiar contenido o claves
cifra primero y sustituye el registro completo mediante una transacción con
control de revisión. Una clave inexistente, revocada, caducada o sin una
subclave utilizable para cifrar no se acepta.

La bóveda empieza bloqueada. Bloquear descarta el detalle y el editor,
oculta el contenido y limpia la copia vigente de KryptoPass del portapapeles.
Los borradores requieren confirmación antes de descartarse. Los guardados
en curso deben terminar; los resultados de lecturas anteriores se invalidan
para impedir que vuelvan a publicar contenido después del bloqueo.

Desbloquear permite solicitar otra lectura: el contenido se vuelve a
descifrar con GnuPG. KryptoPass no cambia `gpg-agent.conf`, los tiempos de
caché ni las autorizaciones del agente, y no lo reinicia. Por ello, GnuPG
puede autorizar una lectura posterior sin volver a pedir la frase de paso.

Los datos tipados y buffers de texto plano propios de Rust usan borrado al
descartarse. Se reduce la proyección de contraseña a QML: en el detalle
solo se incluye al revelarla; editar necesita una copia temporal. Vaciar
`QString`, campos QML y el portapapeles elimina las referencias accesibles,
pero no garantiza borrar todas las copias de Qt, JavaScript, GPGME, el
compositor, otros clientes, swap o volcados de memoria. Una operación de
GnuPG que ya está en curso puede conservar buffers internos hasta terminar;
el bloqueo invalida su resultado sin alterar el agente.

## Portapapeles

La copia de usuario y contraseña usa un componente nativo dedicado. La
caducidad inicial es de 30 segundos; el menú permite cambiarla o desactivarla
con el valor 0. La preferencia se guarda con QSettings.

Cada copia lleva un token aleatorio. Al caducar, bloquear o cerrar, solo se
vacía el portapapeles si aún coinciden token y texto. Copiar algo después,
incluso el mismo texto sin ese token, hace perder la propiedad a KryptoPass.
No se borra contenido posterior del usuario ni se purga su historial.

Se añade el MIME de KDE `x-kde-passwordManagerHint` con valor `secret`.
Es una indicación para gestores de historial que la respeten; no impide que
otros clientes lean el portapapeles activo. La verificación de Qt offscreen
comprueba token, MIME, temporizador, desactivación y limpieza propia. Este
entorno no proporciona una sesión Plasma aislada con Klipper, por lo que
el historial de Klipper y las sesiones X11/Wayland reales quedan pendientes
de comprobación manual. No se declara compatibilidad verificada con ellas.

## Backup v1 y restauración

El archivo `.kpb` es un mensaje OpenPGP. Su contenido es un documento JSON
con `format: "org.kryptopass.backup"`, `version: 1`, `folders` y `secrets`.
También quedan cifrados los nombres, la jerarquía, las apariencias, los
destinatarios, los favoritos, la papelera y las revisiones. Los ciphertexts
originales se transportan en base64; no se vuelven a cifrar individualmente.

Exportar toma una instantánea consistente en una transacción de lectura.
El JSON permanece en memoria y el temporal de publicación contiene solo
ciphertext, con permisos 0600. Se sincroniza antes del rename atómico.
Si el archivo ya se publicó pero falla la sincronización del directorio,
la interfaz informa publicación con durabilidad no confirmada.

Los límites de v1 son 64 MiB para el documento descifrado, 128 MiB + 256 KiB
para el contenedor, 10 000 registros en total, 100 000 referencias a claves,
64 destinatarios por registro y 256 destinatarios para el contenedor.
Excederlos produce un error, sin exportación parcial.

Restaurar valida integridad OpenPGP, versión, campos, tamaños, UUIDs,
duplicados, huellas, referencias y ciclos antes de importar. No se aceptan
mensajes antiguos sin protección de integridad. Se prepara una SQLite
nueva en un directorio privado, se importa en una transacción, se cierra y
se publica sin sobrescribir. El destino debe estar en una carpeta 0700.
La fusión con otra bóveda queda fuera del formato inicial.

El backup no incluye claves privadas. Abrir el contenedor necesita una de
sus claves destinatarias; abrir cada secreto restaurado necesita además
una de las claves originales de ese secreto. Tras restaurar se puede abrir
la bóveda nueva explícitamente. La selección se guarda en la configuración
privada de KryptoPass y no sustituye la bóveda anterior.

## Contactos de KDE

La selección puede partir de contactos asociados a claves existentes.
KryptoPass consulta Akonadi y utiliza los diálogos nativos de KDE para
crear o editar contactos. Vincular muestra el contacto y la huella completa
antes de guardar `X-KADDRESSBOOK-OPENPGPFP` en el contacto. Elegir un
destinatario cruza esa huella con claves válidas del llavero; compartir un
correo no autoriza una clave. No se importan ni se generan claves.

Compilar esta integración requiere CMake y los SDK de Qt Widgets,
KF6 Contacts y KPim6 Akonadi Core, Contact Core y Contact Widgets. CMake
resuelve los paths de los targets instalados. En ejecución se necesita una
agenda Akonadi disponible; abrir la agenda externa requiere KAddressBook.
Los errores de agenda no cambian los destinatarios guardados.

## Evidencia de verificación aislada

Se usaron GNUPGHOME, XDG_DATA_HOME, XDG_CONFIG_HOME y XDG_RUNTIME_DIR
temporales, sin claves, datos, agenda ni portapapeles personales. No se
añadió una suite de tests al repositorio.

- GPGME rechazó claves inexistentes, revocadas, caducadas y de solo firma.
- Crear, guardar, cerrar, reabrir y descifrar funcionó con SQLite real.
- Mover y editar conservaron los destinatarios; claves inválidas y conflictos
  de revisión conservaron ciphertext, claves y revisión anteriores.
- Los paquetes OpenPGP contienen exactamente la clave personal seleccionada
  para el secreto y las dos claves elegidas para el contenedor.
- Una restauración conservó campos, organización, destinatarios, flags y
  revisiones; todos los secretos se abrieron con un segundo llavero limpio.
- Un llavero ajeno no pudo descifrar el contenedor. Cancelar un Pinentry
  aislado produjo un error de descifrado sin devolver texto plano.
- Truncamiento, manipulación, versión incompatible, colisión de destino,
  intento de sobrescribir SQLite y fallo de permisos no dañaron la bóveda.
- Interrumpir una transacción SQLite sin commit conservó los campos y
  revisiones anteriores. Forzar una interrupción de escritura del backup
  conservó el contenedor válido publicado previamente.
- Qt offscreen verificó caducidad, MIME sensible, desactivación y propiedad
  del portapapeles, incluida una copia posterior con texto idéntico.
- El puente real Rust/QML comprobó el arranque bloqueado, descifrado y
  revelado, bloqueo inmediato durante lecturas del detalle y editor,
  rechazo de acciones sensibles mientras está bloqueado y activación de
  otra bóveda con las proyecciones sensibles vacías.

La sincronización con una agenda personal y el historial real de Klipper
no se ejercitaron para evitar modificar datos de la sesión del usuario.
