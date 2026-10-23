# TURN credentials v1 — contrato común

2026-10-19. TURN es respaldo ICE; no autentica peers ni amplía trust P2P.
El emisor de credenciales está separado del relay de señalización y escucha en
loopback detrás de Cloudflare Tunnel. Su lista de acceso concede uso de TURN,
no confianza entre dispositivos. No hay alta pública automática de claves.

## Autenticación del emisor

Origen HTTPS configurado localmente, por defecto de despliegue
`https://signal.kryptopass.org`; nunca lo determina un QR ni el relay remoto.
Origen canónico sin slash, query, usuario o fragmento. Solo tests explícitos
admiten HTTP loopback. Endpoint POST `/kp/turn/v1/credentials`, sin query,
Content-Type application/json, respuesta Cache-Control no-store. Petición
máximo 2048 bytes, exactamente cinco campos, sin duplicados/desconocidos:

```json
{"v":1,"device":"<hex64>","timestamp":1792340000,"nonce":"<hex64>","signature":"<hex128>"}
```

Hex minúscula; nonce aleatorio 32 bytes no cero; timestamp Unix segundos.
Firma Ed25519 con la identidad existente sobre estos bytes concatenados:

1. ASCII `KryptoPass TURN credentials v1` seguido de byte NUL.
2. u32 big-endian de longitud del origen HTTPS ASCII canónico, seguido del origen.
3. device ID 32 bytes.
4. timestamp u64 big-endian.
5. nonce 32 bytes.

El servidor verifica ID = SHA256(domain original DeviceId || signing public
key), clave en allowlist de acceso TURN, firma estricta y tiempo ±60 segundos.
Registra nonces autenticados durante 125 segundos, rechaza repetición y limita
emisiones a tres por dispositivo/minuto, 1024 nonces globales, 16 solicitudes
activas y 64 dispositivos configurados. Fallo de proveedor no concede confianza.
No registra peticiones, claves, firmas, timestamps individuales o credenciales.

## Respuesta

```json
{"v":1,"expires_at":1792343600,"ice_servers":[{"urls":["turn:turn.cloudflare.com:3478?transport=udp","turn:turn.cloudflare.com:443?transport=udp","turn:turn.cloudflare.com:3478?transport=tcp","turns:turn.cloudflare.com:443?transport=tcp"],"username":"<temporal>","credential":"<temporal>"}]}
```

Campos exactos; máximo 16 KiB, hasta cuatro servidores/cuatro URL cada uno,
16 URL total; credenciales por servidor <=1024 bytes combinados, sin controles.
Caducidad >now+30 y <=now+3660; TTL emisor 3600s. Cliente verifica y mantiene
solo en memoria; cache por origen+ID hasta expires_at-120. Nunca persistir token
Cloudflare ni credenciales TURN en APK, preferencias, QR, SDP adicional o logs.
Los usuarios/credenciales ICE estándar viajan al servidor TURN mediante su
protocolo y las credenciales ICE de sesión habituales siguen siendo efímeras.
Respuestas no válidas/errores HTTP no contienen detalles del proveedor.

Solo destinos oficiales `turn.cloudflare.com`, puertos 3478/443 para turn UDP o
TCP y turns TCP443, sintaxis opaca ICE `turn:host:port?transport=udp|tcp`. No
admitir servidores arbitrarios desde una respuesta ni seguir redirecciones.
Timeout cliente 10 segundos, sin proxy/TLS permisivo ni fallback HTTP.

## Integración

Desktop habilita fetch con `KP_TURN_BROKER_ORIGIN`. Su ausencia conserva STUN y
la configuración ICE local existente. `KP_TURN_RELAY_ONLY=1` exige TURN y fuerza
RTCIceTransportPolicy::Relay para la prueba. `KP_INTERNET_ICE_SERVERS_JSON`
manual y broker no se mezclan: configuración explícita manual tiene prioridad;
relay-only requiere al menos un TURN soportado. Firma mediante DeviceIdentity.

Android lee `no_backup/turn-broker-v1.json`, JSON público local acotado
`{"v":1,"origin":"https://signal.kryptopass.org","relay_only":false}`. Ausente
conserva comportamiento anterior. Firma mediante LabIdentity; el controller
obtiene el bundle antes de conectar y lo instala en esa instancia de transporte.
`relay_only` fuerza IceTransportsType.RELAY, nunca entra por QR remoto.

Modo normal: ICE ofrece directo/STUN y TURN juntos; ICE selecciona el camino.
Si el broker no está disponible se conserva STUN/directo con diagnóstico
estático, sin cambios de trust. Modo relay-only: cualquier fallo del broker o
configuración falla, sin fallback directo. LAN nunca solicita TURN.
Credenciales caducadas se renuevan para nuevos intentos; la rotación en vivo de
una sesión larga queda fuera de esta entrega y no debe afirmarse implementada.

webrtc-rs 0.21.1 solo soporta TURN UDP. Desktop filtra URLs TCP/TLS explícitamente
y usa TURN UDP3478/443; Android puede usar las cuatro. ICE-TCP candidates siguen
excluidos: transporte al servidor TURN TCP no implica candidato ICE TCP.
No se garantiza conectividad si el Desktop solo permite salida TCP/TLS.

## Archivos de servidor (privados, fuera del repositorio)

`~/.config/kryptopass-signal/turn-provider.json` 0600:
`{"key_id":"<Cloudflare TURN key ID>","api_token":"<TURN API token>"}`.
`~/.config/kryptopass-signal/turn-access.json` 0600:
`{"v":1,"origin":"https://signal.kryptopass.org","devices":[{"device":"<hex64>","signing_public_key":"<hex64>"}]}`.
Los dispositivos se provisionan administrativamente con sus claves públicas
verificadas. Olvidar un peer en una app no revoca el acceso TURN del otro equipo;
revocar su entrada TURN es otra operación del operador. Credenciales emitidas
pueden seguir válidas hasta expirar/revocarse en proveedor. El backend nunca
importa claves privadas ni convierte esta allowlist en trust P2P.


## Vector canónico compartido Rust/Kotlin

Solo datos sintéticos; no es una petición vigente ni una credencial:

- Origen: `https://signal.kryptopass.org` (29 bytes ASCII).
- Device ID: bytes consecutivos `00` a `1f` (32 bytes).
- Timestamp: `1792340000`.
- Nonce: bytes consecutivos `a0` a `bf` (32 bytes).
- Mensaje a firmar, hexadecimal:

```text
4b727970746f50617373205455524e2063726564656e7469616c73207631000000001d68747470733a2f2f7369676e616c2e6b727970746f706173732e6f7267000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f000000006ad4f020a0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebf
```


## Activación local y reproducción

Desktop (desde el proyecto Desktop, conserva el perfil existente):

```sh
KP_TURN_BROKER_ORIGIN=https://signal.kryptopass.org just pairing-lab
```

Prueba Desktop exclusivamente por relay:

```sh
KP_TURN_BROKER_ORIGIN=https://signal.kryptopass.org KP_TURN_RELAY_ONLY=1 just pairing-lab
```

Android usa el archivo privado público descrito arriba. `relay_only:false`
activa ICE normal con TURN como opción; `true` se reserva a la prueba forzada.
Esta entrega todavía requiere provisionar esa configuración local; el QR y
el relay no habilitan ni cambian el proveedor. La APK no contiene API tokens.

`just turn-smoke` en Desktop exige una respuesta temporal del contrato en stdin.
No pasar username/password por argumentos o variables de entorno, ni guardarlos
en archivos. El test opt-in crea dos PeerConnection Relay, intercambia bytes
binarios en ambos sentidos y verifica el par seleccionado en los dos extremos.
`KP_TURN_SMOKE_BIND_ADDR` puede fijar una dirección local literal con puerto 0
para aislar una ruta; predetermina `0.0.0.0:0`. Las fases de salida son estáticas.
No ejecutarlo contra claves reales OpenPGP ni confundirlo con un pairing Noise:
prueba el transporte TURN, y la prueba de apps añade la identidad y Noise.

Android: `just test-turn <serial-ADB>` ejecuta el contrato con valores sintéticos
mediante el JsonReader real del sistema. Instala in situ y utiliza runner
filtrado; no borra los datos ni crea identidades de vinculación.


## Recolección ICE acotada en Desktop

Cuando hay TURN configurado, Desktop toma el SDP local del motor WebRTC al
completar gathering, recibir un relay admitido o llegar a diez segundos de
recolección. El snapshot pasa la misma validación de SDP/candidatos. En política
Relay se exige al menos un candidato relay y nunca se sustituye por directo.
En política All se pueden utilizar los candidatos directos/STUN ya reunidos.
No se fabrican candidatos ni se promueve confianza por este plazo; Noise y
pinning siguen siendo necesarios después de abrir DataChannel.

Esto evita esperar indefinidamente al gathering completo cuando una interfaz o
endpoint TURN no responde. En webrtc-rs/rtc-turn 0.21.1 el timeout de Allocate
anónimo puede usar un ID distinto del esperado por el relayer para finalizar
gathering. Se conserva la dependencia publicada y se limita la espera del
adaptador. Los treinta segundos totales de negociación y el flujo LAN anterior
se conservan. El snapshot no incorpora candidatos tardíos; no se añade trickle
ICE en esta entrega.


En desarrollo, `just run` exporta
`KP_TURN_BROKER_ORIGIN=https://signal.kryptopass.org` salvo que la variable ya
esté definida, y nunca relay-only. Desde 2026-10-23 no existe lanzador de
desarrollo instalado; una instalación con `just install` no fija la variable. Integrar esta opción
en configuración persistente y UI forma parte del siguiente paso.
