# Identidades y personas en Desktop

Implementado el 2026-11-10. La administración conserva todas las operaciones
OpenPGP existentes en la sección OpenPGP y añade flujos orientados a tareas.

## Modelo

- Identidad: certificado OpenPGP completo, identificado por la huella primaria.
  Su nombre local es independiente de los nombres y correos firmados.
- Componente: clave primaria o subclave; sus capacidades, vigencia y disponibilidad
  se observan en GnuPG. Una referencia a tarjeta no prueba que esté conectada.
- Uso: nombre local y propósito (cifrar, firmar, autenticar) asignados a la huella
  exacta de un componente compatible y vigente. Varias asignaciones pueden usar
  el mismo componente. Nombrar «GitHub» no configura GitHub ni concede permisos.
- Persona: agrupación local con ID propio y varias identidades públicas. No se
  fusionan identidades por correo. La agenda KDE sigue siendo una integración
  opcional, independiente de la agrupación local.
- Comprobación: comparación humana local de la huella; registra fecha y los UID
  observados. Cambiar de persona o cambiar los UID invalida la presentación de
  esa comprobación. No modifica ownertrust ni genera certificaciones OpenPGP.
- Publicación: evidencia por servidor, con envío aceptado, observación de presencia
  y sus fechas. Una respuesta de subida satisfactoria no demuestra disponibilidad
  ni publicación de todos los UID. La consulta comprueba la huella exacta sin
  importar datos al llavero. No existe un estado global «publicada».

Las identidades con material privado se organizan inicialmente como propias;
esta clasificación se conserva si después se retira el material privado. La
persona puede cambiarla explícitamente desde Organizar. Organizar como propia
no equivale a confiar en ella en GnuPG.

## Vista de una clave

Una cabecera única (alias editable, UID, estado y caducidad, huella con copia, y a
la derecha `Copiar clave pública` y `Exportar…`) y una sola barra de pestañas:

1. General y accesos: configuración en KryptoPass (clave de bóveda, olvidar
   contraseña al bloquear), cada clave o subclave real una sola vez con su etiqueta
   local, chips de capacidad, algoritmo, ID, estado y caducidad, y los UID.
2. Seguridad y recuperación: checklist de una línea y zona de peligro.
3. Servidores y firmas: estado por servidor y certificaciones de terceros.
4. Contactos KDE.

Reglas de la interfaz: sin párrafos explicativos (la explicación va en el tooltip
de un `(i)`), sin pestañas anidadas, términos OpenPGP precisos y sin avisos
permanentes. La lista solo marca una clave si está caducada, revocada, inválida o
caduca en 15 días o menos; el respaldo o el certificado pendientes aparecen solo
en el checklist.

`keyring/health.rs` decide los códigos de estado e `IdentityWording.qml` es el
único lugar donde se convierten en texto (`qsTr`); los plurales `%n` se añaden
también en `packaging/translations.py`. La etiqueta de una clave es un registro
local (`setLabel`) ligado a su huella exacta; no modifica la clave. Publicar
consulta la presencia en el mismo paso.

## Flujos

Añadir identidad permite crear, importar o elegir una existente; después asignar
nombre, preferencia de caché y destinatario predeterminado para nuevos elementos.
El asistente está disponible después del primer arranque. Los elementos existentes
no se vuelven a cifrar al cambiar la identidad predeterminada.

Usos permite elegir una tarea y un componente compatible, crear otra subclave y
copiar la clave pública SSH del componente exacto mediante el selector `FPR!` de
GnuPG. Quitar un uso no borra la clave.

Protección separa exportar un respaldo privado, preparar un certificado de
revocación, aplicar/revocar y eliminar una copia local. La fecha de exportación
no acredita existencia actual ni restauración correcta del respaldo. El borrado
solicitado al agente al bloquear incluye las identidades configuradas; no elimina
claves y puede afectar a otras aplicaciones que comparten agente.

Compartir explica la clave pública y la huella. Publicar requiere confirmación
local del destino y del contenido público. HKP funciona sobre HTTPS/HKPS, sin
redirecciones, con plazos y cuerpos limitados. El servidor puede filtrar datos o
requerir verificación de correo en su web. Comparar la versión local con el último
envío solo se realiza al consultar o publicar; no hay monitor en segundo plano ni
promesa de retirar copias de terceros. Servidores retirados de configuración
conservan su historial, pero deben reactivarse para operar.

Referencia del protocolo: <https://keys.openpgp.org/about/api/>.

## Persistencia y límites

`identities.json`, bajo el directorio privado de configuración de la aplicación,
almacena únicamente estos metadatos locales. Es JSON **sin cifrar**, protegido por
permisos del sistema de archivos. La escritura es atómica, limitada a 2 MiB y
serializada entre procesos mediante bloqueo de archivo. Un archivo corrupto o de
versión desconocida produce error; no se reemplaza por un catálogo vacío.
No se guardan claves privadas, contraseñas ni tokens de publicación.

Este catálogo no cambia la bóveda, su esquema ni el contrato Sync y **no viaja al
teléfono**. Tampoco representa ubicaciones remotas no observadas, permisos de
agente/automatización, confirmación remota de accesos SSH ni restauración probada.
Esas capacidades requieren flujos y contratos propios, no etiquetas locales.

La publicación no se ejerce durante la validación de desarrollo: requiere que la
persona decida enviar su identidad al servidor desde la interfaz.
