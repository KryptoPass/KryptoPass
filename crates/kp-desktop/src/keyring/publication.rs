//! Explicit public-key publication over HTTPS HKP. Retrieval never imports into the keyring.
use super::catalog;
use anyhow::{Context, Result, bail};
use kryptopass_keyring::{Fingerprint, Keyring};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Read, time::Duration};

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Publication {
    pub uploaded_at: Option<u64>,
    pub checked_at: Option<u64>,
    pub state: Observation,
    pub uploaded_digest: Option<String>,
    pub current_digest: Option<String>,
}
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Observation {
    #[default]
    Unknown,
    Present,
    Missing,
    Error,
}
fn origin(server: &str) -> Result<url::Url> {
    let normalized = server
        .strip_prefix("hkps://")
        .map(|host| format!("https://{host}"))
        .unwrap_or_else(|| server.to_owned());
    let url = url::Url::parse(&normalized)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        bail!("Para publicar o comprobar, usa un servidor HTTPS o HKPS sin ruta ni credenciales.");
    }
    Ok(url)
}
pub fn run(fingerprint: &str, server: &str, upload: bool) -> Result<catalog::Catalog> {
    let fingerprint = Fingerprint::parse(fingerprint)?;
    let origin = origin(server)?;
    let public = Keyring::system().public_key_armored(&fingerprint)?;
    if public.len() > 1024 * 1024 {
        bail!("La clave pública supera el límite de publicación de 1 MiB.");
    }
    let digest = format!("{:x}", Sha256::digest(public.as_bytes()));
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(25))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let lookup = || -> Result<Observation> {
        let response = client
            .get(origin.join("pks/lookup")?)
            .query(&[("op", "get"), ("search", &format!("0x{fingerprint}"))])
            .send()?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Observation::Missing);
        }
        if !response.status().is_success() {
            bail!("El servidor no pudo completar la consulta.");
        }
        let mut bytes = Vec::new();
        response.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            bail!("La respuesta del servidor supera el límite permitido.");
        }
        let mut context = gpgme::Context::from_protocol(gpgme::Protocol::OpenPgp)?;
        let mut data = gpgme::Data::from_bytes(bytes)?;
        let keys = context.read_keys(&mut data)?;
        let mut found = false;
        for key in keys {
            let key = key?;
            if key.has_secret() {
                bail!("El servidor no devolvió una clave pública válida.");
            }
            if key.fingerprint().ok() == Some(fingerprint.as_str()) {
                found = true;
            }
        }
        if !found {
            bail!("La respuesta no contiene la identidad solicitada.");
        }
        Ok(Observation::Present)
    };
    let result = if upload {
        client
            .post(origin.join("pks/add")?)
            .form(&[("keytext", public.as_str())])
            .send()
            .map_err(anyhow::Error::from)
            .and_then(|response| {
                if !response.status().is_success() {
                    bail!("El servidor no aceptó la publicación. Puedes volver a intentarlo.");
                }
                // The upload is what was asked for; a failed look-up afterwards only leaves
                // the presence unknown until the next check.
                Ok(lookup().unwrap_or(Observation::Unknown))
            })
    } else {
        lookup()
    };
    let successful = result.is_ok();
    let state = result.as_ref().ok().copied();
    let catalog = catalog::update(|catalog| {
        let publication = catalog.identities.entry(fingerprint.to_string()).or_default().publications.entry(server.to_owned()).or_default();
        publication.current_digest = Some(digest.clone());
        if upload && successful {
            publication.uploaded_at = Some(catalog::now());
            publication.uploaded_digest = Some(digest);
        }
        match state {
            Some(Observation::Unknown) => {
                publication.state = Observation::Unknown;
                publication.checked_at = None;
            }
            Some(observed) => {
                publication.checked_at = Some(catalog::now());
                publication.state = observed;
            }
            None if !upload => {
                publication.checked_at = Some(catalog::now());
                publication.state = Observation::Error;
            }
            None => {}
        }
        Ok(())
    }).context("La operación online terminó, pero no se pudo guardar su estado local. Comprueba el servidor antes de repetirla.")?;
    match result {
        Ok(_) => Ok(catalog),
        Err(_) => bail!(
            "No se pudo completar la operación con el servidor. Revisa la conexión y vuelve a intentarlo."
        ),
    }
}
