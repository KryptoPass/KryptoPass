//! Local QR generation and bounded image decoding for authenticator provisioning.

use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    io::{Cursor, Read},
    path::Path,
};
use zeroize::Zeroizing;

pub(super) fn source(payload: &str) -> Result<Zeroizing<String>> {
    let code = qrcode::QrCode::new(payload.as_bytes()).context("QR content exceeds capacity")?;
    let svg = Zeroizing::new(
        code.render::<qrcode::render::svg::Color>()
            .min_dimensions(320, 320)
            .build(),
    );
    let encoded = Zeroizing::new(STANDARD.encode(svg.as_bytes()));
    Ok(Zeroizing::new(format!(
        "data:image/svg+xml;base64,{}",
        encoded.as_str()
    )))
}

pub(super) fn read_totp(path: &Path) -> Result<Zeroizing<String>> {
    const MAX_BYTES: u64 = 8 * 1024 * 1024;
    let file = std::fs::File::open(path)?;
    ensure!(file.metadata()?.is_file(), "select a regular image file");
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "QR image exceeds size limit"
    );
    let mut reader =
        image::ImageReader::new(Cursor::new(bytes.as_slice())).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let mut image = rqrr::PreparedImage::prepare(reader.decode()?.to_luma8());
    let grids = image.detect_grids();
    let mut result = None;
    for grid in grids {
        if let Ok((_metadata, decoded)) = grid.decode() {
            let decoded = Zeroizing::new(decoded);
            if decoded
                .get(.."otpauth://".len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("otpauth://"))
            {
                let config = kryptopass_core::parse_totp_input(&decoded)?
                    .context("QR does not contain a TOTP configuration")?;
                ensure!(
                    result.is_none(),
                    "image contains multiple authenticator codes"
                );
                result = Some(kryptopass_core::format_totp_input(&config)?);
            }
        }
    }
    result.context("image contains no readable authenticator code")
}

#[cfg(test)]
#[path = "secret_qr_tests.rs"]
mod tests;
