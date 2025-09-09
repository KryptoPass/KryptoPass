//! KryptoPass Envelope Encryption

// use anyhow::{bail, Context, Result};
// use argon2::{password_hash::PasswordHasher, Algorithm, Argon2, Params, Version};
// use rand::{rngs::OsRng, RngCore};
// use serde::{Deserialize, Serialize};
// use std::time::{SystemTime, UNIX_EPOCH};

// use kryptopass::kryptography::prelude::*;

// const MAGIC: &[u8; 4] = b"KPB1";
// const VERSION: u32 = 1;

// #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
// #[serde(rename_all = "kebab-case")]
// pub enum AeadSuite {
//     Aes256Gcm, // AeadAlg::AesGcm
//     // Si luego soportas XChaCha20-Poly1305 en tu factory, agrega aquí
// }

// impl AeadSuite {
//     fn to_alg(self) -> AeadAlg {
//         match self {
//             AeadSuite::Aes256Gcm => AeadAlg::AesGcm,
//         }
//     }
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// #[serde(rename_all = "kebab-case")]
// pub struct KdfParamsV1 {
//     pub alg: String, // "argon2id"
//     #[serde(with = "serde_bytes")]
//     pub salt: Vec<u8>, // 16 bytes
//     pub mib: u32,      // memory (MiB)
//     pub t: u32,        // iterations
//     pub p: u32,        // lanes
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// #[serde(tag = "type", rename_all = "kebab-case")]
// pub enum KeySlot {
//     /// Slot basado en contraseña => KEK = Argon2id(password, salt, m/t/p)
//     Password {
//         id: String,
//         kdf: KdfParamsV1,
//         wrap_alg: String, // "aead"
//         #[serde(with = "serde_bytes")]
//         wrap_nonce: Vec<u8>, // 12 bytes
//         #[serde(with = "serde_bytes")]
//         wrapped_dek: Vec<u8>,
//     },
//     /// Slot con KEK directo (passkey/hardware/recovery), ya de 32 bytes
//     Kek {
//         id: String,
//         wrap_alg: String, // "aead"
//         #[serde(with = "serde_bytes")]
//         wrap_nonce: Vec<u8>, // 12 bytes
//         #[serde(with = "serde_bytes")]
//         wrapped_dek: Vec<u8>,
//         // opcionalmente podrías poner "hint" del origen (tpm, keychain, etc.)
//     },
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct KeeHeader {
//     #[serde(with = "serde_bytes")]
//     pub magic: Vec<u8>, // "KPB1"
//     pub version: u32,   // 1
//     pub suite: AeadSuite,
//     pub created: u64,          // unix ts
//     #[serde(with = "serde_bytes")]
//     pub file_id: Vec<u8>,      // 16 bytes
//     #[serde(with = "serde_bytes")]
//     pub data_nonce: Vec<u8>,   // 12 bytes (AES-GCM)
//     pub key_slots: Vec<KeySlot>,
//     pub min_suite: Option<String>, // reservado anti-downgrade
// }

// #[derive(Debug, Clone)]
// pub struct Kee;

// impl Kee {
//     pub fn new() -> Self {
//         Self
//     }

//     /// Crea un backup KEE: HEADER (CBOR) + DATA (AEAD)
//     /// `password_slots`: lista de contraseñas (ej: principal + backup passphrase)
//     /// `kek_slots`: KEKs directos de 32 bytes (ej: passkey/hardware/recovery)
//     pub fn create_backup(
//         &self,
//         plaintext_zip: &[u8],
//         password_slots: &[(&str, KdfTune)], // (slot_id, KdfTune{mib,t,p, salt_len})
//         kek_slots: &[(&str, [u8; 32])],     // (slot_id, KEK)
//     ) -> Result<Vec<u8>> {
//         // 1) Generar DEK y nonces
//         let dek = random_key32();
//         let data_nonce = random_nonce12();
//         let file_id = random_vec(16);

//         // 2) Construir key slots (primero vacíos, luego se completan)
//         let mut slots: Vec<KeySlot> = Vec::new();

//         // 2.a) Slots de contraseña
//         for (slot_id, tune) in password_slots {
//             let salt = random_vec(tune.salt_len.max(16) as usize);
//             let kdf = KdfParamsV1 {
//                 alg: "argon2id".into(),
//                 salt: salt.clone(),
//                 mib: tune.mib,
//                 t: tune.t,
//                 p: tune.p,
//             };
//             // Derivar KEK
//             let kek = derive_kek_argon2id((*slot_id).as_bytes(), &kdf)?;
//             // Wrap de DEK
//             let wrap_nonce = random_nonce12();
//             let wrapped = wrap_dek_aead(&dek, &kek, &wrap_nonce, &file_id, slot_id)?;
//             slots.push(KeySlot::Password {
//                 id: (*slot_id).to_string(),
//                 kdf,
//                 wrap_alg: "aead".into(),
//                 wrap_nonce,
//                 wrapped_dek: wrapped,
//             });
//         }

//         // 2.b) Slots KEK directos
//         for (slot_id, kek) in kek_slots {
//             let wrap_nonce = random_nonce12();
//             let wrapped = wrap_dek_aead(&dek, kek, &wrap_nonce, &file_id, slot_id)?;
//             slots.push(KeySlot::Kek {
//                 id: (*slot_id).to_string(),
//                 wrap_alg: "aead".into(),
//                 wrap_nonce,
//                 wrapped_dek: wrapped,
//             });
//         }

//         // 3) Header
//         let header = KeeHeader {
//             magic: MAGIC.to_vec(),
//             version: VERSION,
//             suite: AeadSuite::Aes256Gcm,
//             created: now_unix(),
//             file_id: file_id.clone(),
//             data_nonce: data_nonce.clone(),
//             key_slots: slots,
//             min_suite: Some("KP-1".into()),
//         };

//         // 4) Serializar header (CBOR)
//         let header_bytes = serde_cbor::to_vec(&header)
//             .context("serializing header")?;

//         // 5) Cifrar datos usando DEK y header como AAD
//         let engine = CryptoFactory::try_aead(header.suite.to_alg())?;
//         let ct = engine.encrypt_checked(
//             &Key32::try_from_slice(&dek)?,
//             &Nonce12::try_from_slice(&data_nonce)?,
//             plaintext_zip,
//             Some(&header_bytes),
//         )?;

//         // 6) Empaquetar: [len_u32_be | header_cbor | ct]
//         let mut out = Vec::with_capacity(4 + header_bytes.len() + ct.len());
//         let len_be: [u8; 4] = (header_bytes.len() as u32).to_be_bytes();
//         out.extend_from_slice(&len_be);
//         out.extend_from_slice(&header_bytes);
//         out.extend_from_slice(&ct);
//         Ok(out)
//     }

//     /// Restaura (descifra) un backup KEE con una credencial.
//     /// `cred`: password(&str) o kek(&[u8;32]).
//     pub fn restore(&self, blob: &[u8], cred: Credential<'_>) -> Result<Vec<u8>> {
//         let (header, header_bytes, ct) = parse_blob(blob)?;
//         let dek = unwrap_dek_from_any_slot(&header, &cred)?;
//         let engine = CryptoFactory::try_aead(header.suite.to_alg())?;
//         let pt = engine.decrypt_checked(
//             &Key32::try_from_slice(&dek)?,
//             &Nonce12::try_from_slice(&header.data_nonce)?,
//             ct,
//             Some(&header_bytes),
//         )?;
//         Ok(pt)
//     }

//     /// Añade un nuevo key slot (p.ej., nueva contraseña) sin re-cifrar DATA.
//     /// Requiere cualquier credencial válida para desenvolver la DEK actual.
//     pub fn add_password_slot(
//         &self,
//         blob: &[u8],
//         auth: Credential<'_>,
//         new_slot_id: &str,
//         new_password: &str,
//         tune: KdfTune,
//     ) -> Result<Vec<u8>> {
//         // parsear
//         let (mut header, _, ct) = parse_blob(blob)?;
//         let dek = unwrap_dek_from_any_slot(&header, &auth)?;

//         // crear nuevo slot
//         let salt = random_vec(tune.salt_len.max(16) as usize);
//         let kdf = KdfParamsV1 {
//             alg: "argon2id".into(),
//             salt: salt.clone(),
//             mib: tune.mib,
//             t: tune.t,
//             p: tune.p,
//         };
//         let kek = derive_kek_argon2id(new_password.as_bytes(), &kdf)?;
//         let wrap_nonce = random_nonce12();
//         let wrapped = wrap_dek_aead(
//             &dek,
//             &kek,
//             &wrap_nonce,
//             &header.file_id,
//             new_slot_id,
//         )?;

//         header.key_slots.push(KeySlot::Password {
//             id: new_slot_id.to_string(),
//             kdf,
//             wrap_alg: "aead".into(),
//             wrap_nonce,
//             wrapped_dek: wrapped,
//         });

//         // re-serializar header y reempaquetar (DATA intacto)
//         let header_bytes = serde_cbor::to_vec(&header)?;
//         let mut out = Vec::with_capacity(4 + header_bytes.len() + ct.len());
//         out.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
//         out.extend_from_slice(&header_bytes);
//         out.extend_from_slice(ct);
//         Ok(out)
//     }

//     /// Sidecar: genera/actualiza un `.kpbkeys` con slots adicionales.
//     /// (El lector consultará primero sidecar y luego header)
//     pub fn make_sidecar_keys(
//         &self,
//         original_blob: &[u8],
//         slots_to_add: &[KeySlot],
//     ) -> Result<Vec<u8>> {
//         let (header, _, _) = parse_blob(original_blob)?;
//         let sidecar = KeeSidecar {
//             file_id: header.file_id,
//             added_slots: slots_to_add.to_vec(),
//         };
//         let bytes = serde_cbor::to_vec(&sidecar)?;
//         Ok(bytes)
//     }
// }

// /* =============== Tipos auxiliares / utilidades =============== */
// #[derive(Debug, Clone, Copy)]
// pub struct KdfTune {
//     pub mib: u32,     // p.ej. 256
//     pub t: u32,       // p.ej. 3
//     pub p: u32,       // p.ej. 1
//     pub salt_len: u32 // >= 16
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct KeeSidecar {
//     #[serde(with = "serde_bytes")]
//     pub file_id: Vec<u8>,
//     pub added_slots: Vec<KeySlot>,
// }

// pub enum Credential<'a> {
//     Password(&'a str),
//     Kek(&'a [u8; 32]),
// }

// fn parse_blob(blob: &[u8]) -> Result<(KeeHeader, Vec<u8>, &[u8])> {
//     if blob.len() < 4 { bail!("blob too small"); }
//     let mut len_be = [0u8; 4];
//     len_be.copy_from_slice(&blob[..4]);
//     let hlen = u32::from_be_bytes(len_be) as usize;
//     if blob.len() < 4 + hlen { bail!("truncated header"); }
//     let header_bytes = blob[4..(4 + hlen)].to_vec();
//     let header: KeeHeader = serde_cbor::from_slice(&header_bytes)
//         .context("parsing header")?;
//     if header.magic.as_slice() != MAGIC { bail!("bad magic"); }
//     if header.version != VERSION { bail!("unsupported version"); }
//     let ct = &blob[(4 + hlen)..];
//     Ok((header, header_bytes, ct))
// }

// fn now_unix() -> u64 {
//     SystemTime::now()
//         .duration_since(UNIX_EPOCH)
//         .unwrap_or_default()
//         .as_secs()
// }

// fn random_vec(n: usize) -> Vec<u8> {
//     let mut v = vec![0u8; n];
//     OsRng.fill_bytes(&mut v);
//     v
// }
// fn random_key32() -> [u8; 32] {
//     let mut k = [0u8; 32];
//     OsRng.fill_bytes(&mut k);
//     k
// }
// fn random_nonce12() -> Vec<u8> {
//     let mut n = vec![0u8; 12];
//     OsRng.fill_bytes(&mut n);
//     n
// }

// fn derive_kek_argon2id(password: &[u8], kp: &KdfParamsV1) -> Result<[u8; 32]> {
//     if kp.alg.to_lowercase() != "argon2id" {
//         bail!("unsupported kdf");
//     }
//     // Argon2 crate usa Params en KiB; convertimos MiB -> KiB
//     let mem_kib = (kp.mib as u32) * 1024;
//     let params = Params::new(mem_kib, kp.t, kp.p, Some(32))
//         .context("argon2 params")?;
//     let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

//     // Hash (salida cruda de 32 bytes)
//     let mut out = [0u8; 32];
//     a2.hash_password_into(password, &kp.salt, &mut out)
//         .context("argon2 hash")?;
//     Ok(out)
// }

// fn wrap_dek_aead(
//     dek: &[u8; 32],
//     kek: &[u8; 32],
//     wrap_nonce: &[u8],
//     file_id: &[u8],
//     slot_id: &str,
// ) -> Result<Vec<u8>> {
//     let engine = CryptoFactory::try_aead(AeadAlg::AesGcm)?;
//     let aad = make_slot_aad(file_id, slot_id);
//     let ct = engine.encrypt_checked(
//         &Key32::try_from_slice(kek)?,
//         &Nonce12::try_from_slice(wrap_nonce)?,
//         dek,
//         Some(&aad),
//     )?;
//     Ok(ct)
// }

// fn unwrap_dek_aead(
//     wrapped: &[u8],
//     kek: &[u8; 32],
//     wrap_nonce: &[u8],
//     file_id: &[u8],
//     slot_id: &str,
// ) -> Result<[u8; 32]> {
//     let engine = CryptoFactory::try_aead(AeadAlg::AesGcm)?;
//     let aad = make_slot_aad(file_id, slot_id);
//     let dek = engine.decrypt_checked(
//         &Key32::try_from_slice(kek)?,
//         &Nonce12::try_from_slice(wrap_nonce)?,
//         wrapped,
//         Some(&aad),
//     )?;
//     let dek: [u8; 32] = dek
//         .try_into()
//         .map_err(|_| anyhow::anyhow!("invalid dek length"))?;
//     Ok(dek)
// }

// fn make_slot_aad(file_id: &[u8], slot_id: &str) -> Vec<u8> {
//     // AAD simple: "KPB1" || file_id || slot_id
//     let mut aad = Vec::with_capacity(4 + file_id.len() + slot_id.len());
//     aad.extend_from_slice(MAGIC);
//     aad.extend_from_slice(file_id);
//     aad.extend_from_slice(slot_id.as_bytes());
//     aad
// }

// fn unwrap_dek_from_any_slot(header: &KeeHeader, cred: &Credential<'_>) -> Result<[u8; 32]> {
//     match cred {
//         Credential::Password(pw) => {
//             // probar sólo slots de password
//             for slot in &header.key_slots {
//                 if let KeySlot::Password { id, kdf, wrap_nonce, wrapped_dek, .. } = slot {
//                     let kek = derive_kek_argon2id(pw.as_bytes(), kdf)?;
//                     if let Ok(dek) = unwrap_dek_aead(
//                         wrapped_dek,
//                         &kek,
//                         wrap_nonce,
//                         &header.file_id,
//                         id,
//                     ) {
//                         return Ok(dek);
//                     }
//                 }
//             }
//             bail!("no valid password slot");
//         }
//         Credential::Kek(kek) => {
//             for slot in &header.key_slots {
//                 match slot {
//                     KeySlot::Kek { id, wrap_nonce, wrapped_dek, .. } => {
//                         if let Ok(dek) = unwrap_dek_aead(
//                             wrapped_dek,
//                             kek,
//                             wrap_nonce,
//                             &header.file_id,
//                             id,
//                         ) {
//                             return Ok(dek);
//                         }
//                     }
//                     _ => continue,
//                 }
//             }
//             bail!("no valid KEK slot");
//         }
//     }
// }
