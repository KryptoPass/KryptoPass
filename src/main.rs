use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use rand::rngs::OsRng;
use rand::RngCore;
use std::{fs, io::Write, path::PathBuf};

use kryptopass::kryptography::{prelude::*, support::Support};

/// Formato de archivo .kp (muy simple):
/// magic(6)="KPASS1", salt(16), alg(1), nonce(12), ad_len(u16 LE), ad[..], ct_len(u32 LE), ct[..]
const MAGIC: &[u8; 6] = b"KPASS1";

#[derive(Parser)]
#[command(name="kpass", version, about="Cofre mínimo con AEAD + derivación de clave")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Cifrar archivo con AEAD (deriva clave con Argon2id desde pass)
    Encrypt {
        #[arg(long, value_name="PATH")]
        in_: PathBuf,
        #[arg(long, value_name="PATH")]
        out: PathBuf,
        /// Frase secreta para derivar la clave (Argon2id)
        #[arg(long)]
        pass: String,
        /// Associated Data (AD) opcional: no se cifra, pero se autentica
        #[arg(long)]
        ad: Option<String>,
        /// Algoritmo AEAD
        #[arg(long, default_value="aes-gcm", value_enum)]
        alg: AeadChoice,
        /// Iteraciones/base de costo (opcional; por defecto razonable)
        #[arg(long, default_value_t = 19)]
        m_cost_log2: u32, // 2^19 KiB = 512 MiB ~ en desktop; ajusta según RAM
        #[arg(long, default_value_t = 3)]
        t_cost: u32,
        #[arg(long, default_value_t = 1)]
        lanes: u32,
    },
    /// Descifrar archivo .kp con AEAD
    Decrypt {
        #[arg(long, value_name="PATH")]
        in_: PathBuf,
        #[arg(long, value_name="PATH")]
        out: PathBuf,
        #[arg(long)]
        pass: String,
    },
    /// Mostrar soporte activo (AEAD/Block/Stream)
    Support,
    /// Demo de modo stream (AES-CTR) para archivos grandes (idempotente)
    Stream {
        #[arg(long, value_name="PATH")]
        in_: PathBuf,
        #[arg(long, value_name="PATH")]
        out: PathBuf,
        /// Key 32 bytes en hex
        #[arg(long, value_name="HEX64")]
        key_hex: String,
        /// IV 16 bytes en hex
        #[arg(long, value_name="HEX32")]
        iv_hex: String,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum AeadChoice {
    #[value(name="aes-gcm")]
    AesGcm,
    #[value(name="chacha20-poly1305")]
    Chacha20Poly1305,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Encrypt { in_, out, pass, ad, alg, m_cost_log2, t_cost, lanes } => {
            encrypt_file(in_, out, &pass, ad.as_deref(), alg, m_cost_log2, t_cost, lanes)
        }
        Cmd::Decrypt { in_, out, pass } => decrypt_file(in_, out, &pass),
        Cmd::Support => {
            print_support();
            Ok(())
        }
        Cmd::Stream { in_, out, key_hex, iv_hex } => stream_apply(in_, out, &key_hex, &iv_hex),
    }
}

fn print_support() {
    let s = Support::active();
    print!("AEAD:");
    s.aead.iter().for_each(|s| print!("{s}, "));
    println!();
    print!("Block:");
    s.block.iter().for_each(|s| print!("{s}, "));
    println!();
    print!("Stream:");
    s.stream.iter().for_each(|s| print!("{s}, "));
    println!();
}

fn encrypt_file(
    in_path: PathBuf,
    out_path: PathBuf,
    pass: &str,
    ad: Option<&str>,
    alg: AeadChoice,
    m_cost_log2: u32,
    t_cost: u32,
    lanes: u32,
) -> Result<()> {
    // leer plaintext
    let pt = fs::read(&in_path).with_context(|| format!("leyendo {:?}", in_path))?;

    // generar salt y nonce
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);

    // derivar clave con Argon2id
    let key = derive_key(pass.as_bytes(), &salt, m_cost_log2, t_cost, lanes)?;

    // elegir AEAD
    let (alg_byte, engine) = match alg {
        AeadChoice::AesGcm => (1u8, CryptoFactory::try_aead(AeadAlg::AesGcm)?),
        AeadChoice::Chacha20Poly1305 => (2u8, CryptoFactory::try_aead(AeadAlg::ChaCha20Poly1305)?),
    };

    let key32 = Key32::try_from_slice(&key)?;
    let nonce = Nonce12::try_from_slice(&nonce_bytes)?;
    let ad_bytes = ad.map(|s| s.as_bytes());

    let ct = engine.encrypt_checked(&key32, &nonce, &pt, ad_bytes)?;

    // empaquetar .kp
    let mut out = Vec::with_capacity(
        MAGIC.len() + 16 + 1 + 12 + 2 + ad_bytes.map_or(0, |a| a.len()) + 4 + ct.len(),
    );
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.push(alg_byte);
    out.extend_from_slice(&nonce_bytes);
    let ad_len: u16 = ad_bytes.map_or(0, |a| a.len() as u16);
    out.extend_from_slice(&ad_len.to_le_bytes());
    if let Some(a) = ad_bytes {
        out.extend_from_slice(a);
    }
    let ct_len: u32 = ct.len() as u32;
    out.extend_from_slice(&ct_len.to_le_bytes());
    out.extend_from_slice(&ct);

    // escribir
    write_atomic(&out_path, &out)?;
    println!("OK: cifrado -> {:?}", out_path);
    Ok(())
}

fn decrypt_file(in_path: PathBuf, out_path: PathBuf, pass: &str) -> Result<()> {
    let blob = fs::read(&in_path).with_context(|| format!("leyendo {:?}", in_path))?;
    let mut p = 0;

    // magic
    if blob.len() < MAGIC.len() || &blob[0..MAGIC.len()] != MAGIC {
        return Err(anyhow!("formato inválido (magic)"));
    }
    p += MAGIC.len();

    // salt(16), alg(1), nonce(12)
    ensure_len(&blob, p, 16 + 1 + 12)?;
    let salt = &blob[p..p + 16];
    p += 16;
    let alg_byte = blob[p];
    p += 1;
    let nonce_bytes = &blob[p..p + 12];
    p += 12;

    // ad_len(u16), ad[..]
    ensure_len(&blob, p, 2)?;
    let ad_len = u16::from_le_bytes([blob[p], blob[p + 1]]) as usize;
    p += 2;
    ensure_len(&blob, p, ad_len)?;
    let ad = if ad_len > 0 { Some(&blob[p..p + ad_len]) } else { None };
    p += ad_len;

    // ct_len(u32), ct[..]
    ensure_len(&blob, p, 4)?;
    let ct_len = u32::from_le_bytes([blob[p], blob[p + 1], blob[p + 2], blob[p + 3]]) as usize;
    p += 4;
    ensure_len(&blob, p, ct_len)?;
    let ct = &blob[p..p + ct_len];

    // derivar clave
    let key = derive_key(pass.as_bytes(), array_ref!(salt, 0, 16), 19, 3, 1)?; // defaults; no afectan compat
    let key32 = Key32::try_from_slice(&key)?;
    let nonce = Nonce12::try_from_slice(array_ref!(nonce_bytes, 0, 12))?;

    // elegir engine
    let engine = match alg_byte {
        1 => CryptoFactory::try_aead(AeadAlg::AesGcm)?,
        2 => CryptoFactory::try_aead(AeadAlg::ChaCha20Poly1305)?,
        _ => return Err(anyhow!("algoritmo desconocido en archivo: {}", alg_byte)),
    };

    let pt = engine.decrypt_checked(&key32, &nonce, ct, ad)?;
    write_atomic(&out_path, &pt)?;
    println!("OK: descifrado -> {:?}", out_path);
    Ok(())
}

fn stream_apply(in_path: PathBuf, out_path: PathBuf, key_hex: &str, iv_hex: &str) -> Result<()> {
    // parse key/iv hex
    let key_bytes = hex::decode(key_hex.trim()).context("key hex inválido")?;
    if key_bytes.len() != 32 {
        return Err(anyhow!("la key debe ser 32 bytes (64 hex)"));
    }
    let iv_bytes = hex::decode(iv_hex.trim()).context("iv hex inválido")?;
    if iv_bytes.len() != 16 {
        return Err(anyhow!("el IV debe ser 16 bytes (32 hex)"));
    }
    let key = Key32::try_from_slice(&key_bytes)?;
    let iv = Iv16::try_from_slice(&iv_bytes)?;

    // checar soporte
    if !StreamAlg::AesCtr.is_supported() {
        return Err(anyhow!("AES-CTR no soportado en esta build"));
    }
    let s = CryptoFactory::try_stream(StreamAlg::AesCtr)?;

    let data = fs::read(&in_path)?;
    let out = s.apply_keystream_checked(&key, &iv, &data)?;
    write_atomic(&out_path, &out)?;
    println!("OK: stream -> {:?}", out_path);
    Ok(())
}

fn derive_key(pass: &[u8], salt: &[u8; 16], m_cost_log2: u32, t_cost: u32, lanes: u32) -> Result<[u8; 32]> {
    use argon2::{Algorithm, Argon2, Params, Version};
    let m_cost_kib = 1u32.checked_shl(m_cost_log2).ok_or_else(|| anyhow!("m_cost_log2 muy grande"))?;
    let params = Params::new(m_cost_kib, t_cost, lanes, Some(32)).context("params Argon2")?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; 32];
    argon.hash_password_into(pass, salt, &mut out).context("argon2")?;
    Ok(out)
}

fn write_atomic(path: &PathBuf, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.flush()?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

// helper: bounds check
fn ensure_len(buf: &[u8], pos: usize, need: usize) -> Result<()> {
    if buf.len() < pos + need {
        Err(anyhow!("archivo truncado/corrupto"))
    } else {
        Ok(())
    }
}

/// Mini macro para tomar array ref fija
#[macro_export]
macro_rules! array_ref {
    ($arr:expr, $offset:expr, $len:expr) => {{
        if $arr.len() < $offset + $len {
            panic!("array_ref fuera de rango");
        }
        unsafe {
            &*($arr.as_ptr().add($offset) as *const [u8; $len])
        }
    }};
}
