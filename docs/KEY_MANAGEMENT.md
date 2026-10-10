# Gestión de claves OpenPGP

KryptoPass gestiona el llavero local de OpenPGP con GnuPG mediante GPGME. QML entrega solicitudes JSON a `KeyringController.runOperation(requestJson)` y recibe metadatos de claves, estados de operación y, solo para `copyPublic`, el armor público. El material privado no se proyecta a QML. KryptoPass no pide ni pasa frases de paso: las operaciones que las necesitan usan pinentry de GnuPG. La aplicación no comprueba ni garantiza que una clave ya tenga frase de paso.

## Contrato de operaciones

El invokable `KeyringController.runOperation(requestJson)` acepta un objeto JSON con `action` en camelCase. El esquema es estricto, rechaza campos desconocidos y limita la solicitud a 64 KiB. Los campos `fingerprint` y `signerFingerprint` identifican huellas completas de claves primarias; `subkey` y cada elemento de `subkeys` identifican huellas completas de subclaves.

| `action` | Campos | Resultado y efecto |
| --- | --- | --- |
| `create` | `userid`, `expiresDays` | Crea una clave primaria propia con capacidades de firma y certificación, y añade una subclave de cifrado si no hay una sin revocar. La creación de la primaria y la subclave son pasos separados: si falla el segundo, la primaria permanece en el llavero y no se revierte. |
| `import` | `path` | Importa desde un archivo local. |
| `exportPublic` | `fingerprint`, `path` | Escribe la clave pública en un archivo local nuevo con permisos `0600`. |
| `copyPublic` | `fingerprint` | Devuelve el bloque público ASCII-armored por `publicKeyReady`; no incluye datos privados. |
| `backupPrivate` | `fingerprint`, `path` | Escribe la copia privada en un archivo local nuevo con permisos `0600`; nunca la devuelve a QML. |
| `passphrase` | `fingerprint` | Abre el pinentry de GnuPG para establecer o cambiar la frase de paso. |
| `expiry` | `fingerprint`, `subkeys`, `expiresDays` | Si `subkeys` está vacío, cambia solo el vencimiento de la clave primaria. Si contiene huellas, cambia solo esas subclaves; las demás no se modifican. |
| `addIdentity` | `fingerprint`, `userid` | Añade una identidad. |
| `primaryIdentity` | `fingerprint`, `userid` | Marca una identidad como principal. |
| `revokeIdentity` | `fingerprint`, `userid` | Revoca una identidad. |
| `addSubkey` | `fingerprint`, `usage`, `expiresDays` | Añade una subclave con `usage` igual a `encrypt`, `sign` o `authenticate`. |
| `revokeSubkey` | `fingerprint`, `subkey` | Revoca una subclave identificada por su fingerprint completo. |
| `prepareRevocation` | `fingerprint`, `path` | Escribe el certificado de revocación a un archivo local nuevo. |
| `revokeKey` | `fingerprint` | Revoca la clave propia en el llavero local después de la confirmación de la interfaz; no la publica en servidores. |
| `certify` | `fingerprint`, `signerFingerprint`, `userids`, `local` | Certifica las identidades seleccionadas con la clave firmante indicada. |
| `refreshExternal` | `fingerprint` | Si la clave ya está en el llavero local, la actualiza desde la fuente de claves configurada para GnuPG/dirmngr; si no está, intenta recibirla desde esa fuente. |
| `delete` | `fingerprint`, `material` | Elimina material `public`, `private` o `both`, según el valor confirmado por la interfaz. |

El resultado interno contiene `message`, `selected_fingerprint` opcional y `public_key` opcional. Solo `copyPublic` puede establecer `public_key`. La interfaz serializa una operación a la vez con `operationBusy`; después de cada intento vuelve a consultar metadatos del llavero, incluso si la operación informó un error, porque algunas acciones pueden dejar cambios parciales. La recarga fallida se comunica como advertencia de actualización y no se presenta como si la mutación hubiera fallado.

`expiresDays: 0` significa que no se establece caducidad; la interfaz lo presenta como «Sin caducidad». En `expiry`, eso se aplica al ámbito seleccionado por `subkeys`, y en `create` o `addSubkey`, al material que se está creando.

Los campos `path` llegan al backend como rutas locales nativas. El puente admite URLs `file:` locales y las convierte con Qt; rechaza URLs remotas, rutas vacías y rutas con consulta o fragmento.

## Preferencias de claves en KDE Contacts

La pestaña Crypto Settings de KAddressBook guarda las huellas OpenPGP en el campo de contacto `app = "KADDRESSBOOK"`, `name = "OPENPGPFP"`, con varias huellas separadas por comas. KryptoPass también lee huellas completas de líneas enteras `OpenPGP: <FPR>` en las notas y de URI `openpgp4fpr:<FPR>` de 40 dígitos hexadecimales cuando aparecen como tokens delimitados. El prefijo `OpenPGP:` es una convención de compatibilidad propia de KryptoPass, no un formato estándar de KDE ni de OpenPGP. No se extraen huellas hexadecimales sueltas. Las huellas se normalizan a mayúsculas y se deduplican.

El puente nativo consulta contactos mediante Akonadi y proyecta su UID, nombre, correo preferido y las huellas válidas y completas en `openPgpFingerprints[]`; conserva `openPgpFingerprint` como alias de la primera para clientes anteriores. Solo proyecta huellas, no el texto de las notas. Esta integración no lee eventos del calendario ni guarda o consulta material de claves en KDE Contacts: solo mantiene referencias a huellas; las claves están en el llavero local de GnuPG.

Al vincular una clave, un selector QtWidgets lista los items de contacto obtenidos de Akonadi y permite una sola selección por UID; también incluye contactos sin correo. Tras la confirmación, KryptoPass conserva el texto crudo existente de `OPENPGPFP` y añade la huella si falta. También mantiene al final de las notas un bloque deduplicado de líneas completas `OpenPGP: <FPR>` para facilitar la sincronización con servicios y dispositivos que sincronicen ese campo. Conserva el contenido no marcador; solo elimina y reubica líneas completas `OpenPGP: <FPR>` o una URI independiente `openpgp4fpr:<FPR>` de 40 dígitos. Una URI `openpgp4fpr:` mezclada con otro texto se lee si está delimitada, pero nunca se elimina ni se reescribe como parte del texto. La interfaz Crypto Settings del editor de KDE Contacts sigue siendo el lugar para revisar o retirar la preferencia KDE. El nombre y el correo se muestran para identificar el contacto; la selección se conserva por UID y no resuelve claves ni establece confianza.

Para elegir un destinatario de cifrado, la interfaz ofrece solo claves locales vigentes con capacidad de cifrado. KryptoPass intersecta ese conjunto con las huellas completas combinadas de `OPENPGPFP` y las notas. No resuelve IDs cortos, no busca claves por coincidencia de correo y no trata el correo de un UID como prueba de identidad. KMail también define una preferencia de contacto basada en `OPENPGPFP`; su resolución de destinatarios y sus reglas de confianza son independientes.

## Custodia y límites

Las frases de paso se capturan en pinentry de GnuPG; no hay campos para ellas en las solicitudes ni en QML. La acción `passphrase` abre GnuPG para establecer o cambiarla. La aplicación no comprueba que las claves existentes tengan frase de paso. QML solo recibe armor público para `copyPublic`; la clave privada permanece bajo el control de GnuPG.

Las exportaciones se publican como archivos nuevos con permisos `0600` y no sobrescriben un destino existente. La copia privada conserva la protección que ya tenga el material de origen, que puede no tener frase de paso; KryptoPass no agrega una. Una clave en tarjeta o representada por un stub puede no ser exportable, y material exportado sin la primaria offline puede no bastar para recuperar la clave. No se garantiza que una copia privada pueda restaurarse.

Las preferencias se almacenan en KDE Contacts/Akonadi. La lista contiene referencias a claves públicas, no copias de claves ni evidencia de que la identidad de una persona haya sido verificada por un canal independiente. KryptoPass consulta los contactos cuando se solicita la lista de destinatarios y solo añade una huella al contacto elegido después de confirmación explícita.

`refreshExternal` usa la fuente configurada por GnuPG/dirmngr, que puede compartirse con otras aplicaciones que usen esa configuración. La actualización es una acción explícita de recepción; KryptoPass no envía ni publica claves o revocaciones automáticamente.

La documentación de KDE describe la pestaña [Crypto Settings de KAddressBook](https://docs.kde.org/stable_kf6/en/kaddressbook/kaddressbook/edit-advanced-info.html). El formato de `OPENPGPFP` puede consultarse en el código de [KAddressBook](https://github.com/KDE/kdepim-addons/blob/master/kaddressbook/editorpages/cryptopageplugin.cpp) y en el lector `ContactPreference` de [KDE MessageComposer](https://github.com/KDE/messagelib/blob/master/messagecomposer/src/contactpreference/contactpreference.cpp).
