# Tipos de elementos

El selector ofrece inicio de sesión, nota, secreto personalizado, tarjeta,
autenticador TOTP, identidad, red Wi-Fi, licencia de software y clave SSH.
Los IDs persistentes son `login`, `note`, `custom`, `card`, `totp`, `identity`,
`wifi`, `license` y `ssh-key`. `api-token` sigue reservado; no se convierte
silenciosamente en un secreto personalizado.

## Contenido y acciones

| Tipo | Campos cifrados | Acciones |
| --- | --- | --- |
| Personalizado | Filas ordenadas `id`, `key`, `value`, `multiline`, `concealed`; notas | Agregar, quitar, reordenar, revelar y copiar cada valor |
| Tarjeta | Titular, número, mes/año de vencimiento, CVV, PIN opcional y notas | Revelar número/CVV/PIN y copiar por campo |
| TOTP | Configuración completa y notas | Código actual/siguiente, contador, lectura de QR de imagen y QR de configuración |
| Identidad | Nombre, apellidos, documentos repetibles, correos, teléfonos, dirección y notas | Copiar y revelar datos personales por campo |
| Wi-Fi | SSID, seguridad, contraseña, red oculta y notas | Copiar contraseña y mostrar/ocultar QR |
| Licencia | Producto, clave, titular, correo, versión, compra, vencimiento y notas | Revelar/copiar clave y consultar los otros campos |
| SSH | Privada OpenSSH, frase de contraseña, pública/huella/algoritmo derivados, usuario, hosts y notas | Importar/reemplazar privada desde Rust; copiar pública y datos derivados |

Cada fila personalizada tiene un UUID estable. Se permiten nombres repetidos,
se preservan espacios y saltos de línea, y cambiar la presentación no recorta
el valor. Hasta 100 filas, 512 bytes por nombre y 256 KiB por valor; el sobre
completo sigue limitado a 1 MiB. Los nombres de campos también se cifran.

`concealed` controla la proyección del detalle; no cambia el cifrado. Un campo
oculto llega sin valor a QML hasta que se solicita revelarlo. Copiar un campo
oculto toma su valor del contenido autorizado en Rust, sin revelarlo. La lista
global sigue usando solo metadatos. Cerrar el detalle, bloquear o cambiar de
bóveda invalida revelados y QR. Las copias usan el portapapeles sensible existente.

Las claves SSH se leen en un worker desde un archivo local acotado. Solo el token
de importación y los datos públicos llegan al formulario. El token pertenece
al editor abierto y deja de servir cuando se cierra o cambia la sesión. Editar
usuario, hosts o notas conserva la privada anterior sin proyectarla a QML.
Se admiten claves privadas OpenSSH cifradas con su frase de contraseña o sin
cifrar; otros formatos PEM necesitan conversión previa. La integración como
agente SSH es una extensión posterior.

Wi-Fi admite redes abiertas, WEP, WPA/WPA2 y WPA3. El QR interoperable se ofrece
para las tres primeras: el formato habitual de conexión Wi-Fi no distingue
WPA3, por lo que ese caso muestra una explicación y permite copiar los campos.

## Configuración TOTP

Se aceptan semilla Base32 y URI `otpauth://totp/...`. El lector interpreta
`secret`, `issuer`, etiqueta/cuenta, `algorithm`, `digits` y `period`, conserva
la etiqueta URI original y las extensiones desconocidas dentro del payload.
Formatear para editar y volver a guardar conserva esas extensiones, aunque no
se muestren en el detalle. Los parámetros duplicados, escapes inválidos,
emisores contradictorios y algoritmos o dígitos no admitidos se rechazan sin
sustituirlos por valores por defecto. HOTP se reconoce y se rechaza como una
configuración distinta; todavía no hay ejecución ni almacenamiento HOTP.

Los algoritmos admitidos son SHA-1, SHA-256 y SHA-512, con 6 u 8 dígitos. Los
periodos admitidos son enteros positivos de segundos representables en `u64`;
los cálculos de tiempos comprueban desbordamientos. Los valores por defecto
siguen siendo SHA-1, 6 y 30 segundos. Inicio de sesión y
autenticador independiente comparten lector y motor. El detalle recibe códigos,
emisor, cuenta, algoritmo, dígitos y tiempos; no recibe semilla ni URI. El QR de
configuración se genera en Rust solo al solicitarlo y representa esa semilla,
por lo que tiene el mismo ciclo temporal de autorización que los revelados.

La lectura de QR usa una imagen PNG/JPEG local: hasta 8 MiB, lados de hasta
4096 píxeles y presupuesto de decodificación de 64 MiB. Se exige un único QR
TOTP válido. No se utilizan cámara, red ni archivos temporales de secretos.

## Versiones y compatibilidad

Los nuevos contenidos se escriben con sobre versión 4. Los logins y notas de
versiones 1, 2 y 3 siguen siendo legibles; el siguiente guardado usa 4. Los nuevos
tipos requieren versión 4. SQLite almacena el tipo mediante su ID estable y el
contenido mediante el mismo ciphertext OpenPGP, sin nuevas columnas de texto
sensible. Los respaldos transportan estos ciphertexts con el formato existente.
Un cliente anterior puede rechazar tipos y contenidos nuevos; no debe abrir y
guardar una bóveda nueva desde una versión anterior de la aplicación.

Las recetas `just verify`, `just test-items` y `just smoke` cubren compilación,
lints, contratos Rust/QML y arranque offscreen. Las pruebas usan datos sintéticos.
