# Integración con navegador

El crate `kryptopass-browser` contiene el servidor de socket Unix, el protocolo
cifrado compatible con TweetNaCl y el binario `kryptopass-browser-proxy`. No
depende de Qt, GPGME ni del dominio de secretos. Desktop suministra identidad,
versión, estado de bloqueo, decisiones explícitas y un adaptador de lectura de
credenciales. Los checkouts vecinos
de KeePassXC-Browser y keepassxc-proxy-rust sirven como referencias upstream;
el proxy de producto se construye dentro del workspace.

## Alcance disponible

- Host y extensión propios, con identidad definida en `Cargo.toml`.
- Intercambio de claves efímeras por conexión: X25519 + XSalsa20-Poly1305
  (`crypto_box::SalsaBox`), compatible con el `nacl.box` de la extensión.
- `get-databasehash`, `associate`, `test-associate` y `get-logins`.
- Confirmación de vinculación en Desktop, con rechazo por defecto, caducidad
  de 60 segundos y cancelación al bloquear o cambiar de bóveda.
- Identidad y vinculaciones persistidas por ruta canónica de bóveda. Reiniciar
  conserva la asociación; cambiar o mover la bóveda exige otra asociación.
- Autorización independiente para cada solicitud de credenciales. Desktop
  presenta el origen solicitado y nombres de credenciales; el usuario elige
  una. Solo entonces se descifra esa entrada, fuera del hilo de Qt.
- Coincidencia exacta de esquema HTTP(S), host y puerto efectivo con una URL
  guardada. Las rutas pueden diferir; los subdominios deben estar guardados
  explícitamente. Un formulario con envío a otro origen se rechaza.
- Invalidación por bloqueo o cambio de bóveda y avisos de estado a la extensión.

No se implementan escritura de credenciales, TOTP, generación de contraseñas
ni passkeys desde el navegador. Las acciones no implementadas se rechazan.
El branding visual upstream se conserva provisionalmente.

## Preparar y probar en Linux

Para Brave Origin, desde el workspace de Desktop:

```bash
just browser-fork ../keepassxc-browser
just browser-build
```

En el checkout vecino de la extensión, selecciona el manifiesto Chromium con
`npm run debug:chromium`. En `brave://extensions`, activa el modo de desarrollador
y carga la carpeta `keepassxc-browser` como extensión descomprimida. Copia el ID
que Brave asigna al fork y registra ese ID:

```bash
just browser-install-dev --browser brave-origin --extension-id ID_DE_TU_EXTENSION
just run --browser-integration
```

El manifiesto se instala en
`$XDG_CONFIG_HOME/BraveSoftware/Brave-Origin/NativeMessagingHosts`, con respaldo
en `$HOME/.config`. Usa `--browser brave` para Brave convencional y
`--browser chromium` para Chromium. La lista `allowed_origins` contiene
únicamente `chrome-extension://ID_DE_TU_EXTENSION/`; no se reutiliza el ID
KeePassXC de la tienda.

Desbloquea la bóveda en Desktop, inicia la conexión desde la extensión y
confirma la vinculación en Desktop. Si vienes de la implementación con
asociaciones en memoria, elimina la conexión anterior en los ajustes de la
extensión y vuelve a vincular una vez.

Guarda en la credencial una URL completa, por ejemplo `https://example.com`.
Abre el formulario del sitio y pulsa el icono de relleno de la extensión.
En Desktop elige la credencial y pulsa «Permitir relleno»; GnuPG puede pedir
confirmación mediante pinentry. La extensión recibirá únicamente esa entrada
si su URL coincide. Si la URL falta o es de otro sitio, la solicitud falla;
corrige la credencial y vuelve a solicitar el relleno.

`browser-fork` actualiza el destino del host, el ID de
Firefox y las capacidades del fork; puede volver a ejecutarse.

Para Firefox, selecciona el manifiesto Firefox con `npm run debug:firefox`
en el checkout de la extensión, registra con
`just browser-install-dev --browser firefox` y carga temporalmente
`keepassxc-browser/manifest.json` desde `about:debugging`.

El instalador escribe únicamente el registro del usuario para el navegador
elegido. Rechaza registros existentes para evitar reemplazar otra instalación.
Para retirarlo elimina el manifiesto que imprime el instalador y el archivo
`*-launcher` junto a él. No modifica registros KeePassXC. No se cubren aún
Chrome, Windows, macOS ni navegadores en sandbox/Flatpak. `XDG_RUNTIME_DIR`
debe existir y ser privado.

## Transporte y límites

Browser ↔ proxy y proxy ↔ Desktop usan JSON UTF-8 precedido por longitud `u32`
en orden nativo de bytes. Este framing del socket es propio de KryptoPass: el
proxy upstream quita la longitud antes de enviarla al socket KeePassXC. Ambos
extremos KryptoPass conservan la longitud, por lo que las lecturas fragmentadas
o varios mensajes juntos no cambian sus límites.

Se limita cada frame a 1 MiB antes de reservar memoria, a ocho conexiones y a
32 asociaciones por bóveda. Una conexión inactiva caduca a los 30 minutos.
Las claves de sesión permanecen en Rust y los buffers descifrados se limpian
con `zeroize`; QML recibe tokens, el origen solicitado y metadatos de entradas,
nunca los valores enviados al navegador.
Se rechazan nonces repetidos; tras 4096 solicitudes se exige un nuevo
intercambio de claves. Los nonces de respuesta se incrementan en little-endian,
como hace la extensión.

El socket vive en `$XDG_RUNTIME_DIR/<app-id>/browser.sock`, con directorio
privado y permisos `0600`. Un segundo servidor no reemplaza el socket existente.
Si el proceso terminó abruptamente, comprueba que no esté ejecutándose antes
de retirar un socket antiguo. El servidor solo se activa con
`--browser-integration`; el arranque normal no lo expone.

## Persistencia y contenido privado

Los archivos `browser-<sha256-de-ruta-canónica>.json` viven en el directorio
privado de datos de la aplicación. Guardan una identidad aleatoria de bóveda
y hasta 32 claves públicas de asociación; no contienen usuarios, contraseñas
ni URLs. Se exige directorio `0700` y archivo `0600`, propiedad del usuario,
sin enlaces simbólicos; la escritura es atómica. Un archivo inválido o un
fallo al persistir impide confirmar la asociación.

Las URLs permanecen dentro del contenido OpenPGP. Por eso esta versión no
busca coincidencias descifrando toda la bóveda: solicita elegir una entrada
por nombre y comprueba su URL después de la aprobación. Se excluyen papelera
y tipos distintos de credencial y se comprueba que la revisión no cambió
antes y después de descifrar. Cada solicitud expira a los 60 segundos.

Una vez enviada, la credencial está en el navegador y en la página que la
recibe. Los avisos de bloqueo permiten a la extensión invalidar su caché;
no retiran valores ya rellenados en una página. El contrato completo está en
[SECRET_DATA_CONTRACT.md](SECRET_DATA_CONTRACT.md).
