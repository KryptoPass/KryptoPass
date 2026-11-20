#![allow(clippy::unwrap_used)]

use std::{fs, path::PathBuf};

use base64::Engine;
use image::{DynamicImage, GrayImage, ImageFormat, Luma};
use kryptopass_core::{format_totp_input, parse_totp_input};
use qrcode::{Color, QrCode};
use uuid::Uuid;

use super::{read_totp, source};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kryptopass-qr-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn qr_image(payload: &str, scale: u32) -> GrayImage {
    let code = QrCode::new(payload.as_bytes()).unwrap();
    let quiet_zone = scale * 4;
    let side = code.width() as u32 * scale + quiet_zone * 2;
    let mut image = GrayImage::from_pixel(side, side, Luma([255]));
    draw_qr(&mut image, &code, quiet_zone, quiet_zone, scale);
    image
}

fn draw_qr(image: &mut GrayImage, code: &QrCode, left: u32, top: u32, scale: u32) {
    for y in 0..code.width() {
        for x in 0..code.width() {
            if code[(x, y)] != Color::Dark {
                continue;
            }
            let x0 = left + x as u32 * scale;
            let y0 = top + y as u32 * scale;
            for dy in 0..scale {
                for dx in 0..scale {
                    image.put_pixel(x0 + dx, y0 + dy, Luma([0]));
                }
            }
        }
    }
}

fn save_qr(path: &std::path::Path, payload: &str, format: ImageFormat) {
    let image = qr_image(payload, 10);
    DynamicImage::ImageLuma8(image)
        .save_with_format(path, format)
        .unwrap();
}

fn sample_uri(account: &str, issuer: &str) -> String {
    format!(
        "otpauth://totp/{issuer}%3A{account}?secret=JBSWY3DPEHPK3PXP&issuer={issuer}&algorithm=SHA256&digits=8&period=45&vendor%20key=alpha%2Bbeta"
    )
}

#[test]
fn qr_source_is_a_valid_svg_data_uri_and_does_not_expose_the_payload_as_text() {
    let payload = sample_uri("ada@example.invalid", "Example");
    let image = source(&payload).unwrap();
    let encoded_svg = image.strip_prefix("data:image/svg+xml;base64,").unwrap();
    let svg = base64::engine::general_purpose::STANDARD
        .decode(encoded_svg)
        .unwrap();
    let svg = std::str::from_utf8(&svg).unwrap();

    let svg = svg.trim_start();
    let svg = if let Some(declaration) = svg.strip_prefix("<?xml") {
        let Some(declaration_end) = declaration.find("?>") else {
            panic!("SVG XML declaration should be terminated");
        };
        assert!(declaration[..declaration_end].contains("version="));
        declaration[declaration_end + 2..].trim_start()
    } else {
        svg
    };
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("width=") && svg.contains("height="));
    assert!(svg.trim_end().ends_with("</svg>"));
    assert!(!image.contains("JBSWY3DPEHPK3PXP"));
}

#[test]
fn png_and_jpeg_qrs_decode_to_the_same_valid_totp_configuration() {
    let directory = TestDirectory::new();
    let input = sample_uri("ada@example.invalid", "Example");
    let expected_config = parse_totp_input(&input).unwrap().unwrap();
    let expected = format_totp_input(&expected_config).unwrap();

    for (name, format) in [
        ("authenticator.png", ImageFormat::Png),
        ("authenticator.jpg", ImageFormat::Jpeg),
    ] {
        let path = directory.path(name);
        save_qr(&path, &input, format);
        let actual = read_totp(&path).unwrap();
        assert_eq!(actual.as_str(), expected.as_str());

        let reparsed = parse_totp_input(actual.as_str()).unwrap().unwrap();
        assert!(reparsed == expected_config);
        assert_eq!(reparsed.extensions[0].key, "vendor key");
        assert_eq!(reparsed.extensions[0].value, "alpha+beta");
    }
}

#[test]
fn image_with_multiple_authenticator_codes_is_rejected() {
    let directory = TestDirectory::new();
    let first = QrCode::new(sample_uri("first@example.invalid", "Example").as_bytes()).unwrap();
    let second = QrCode::new(sample_uri("second@example.invalid", "Example").as_bytes()).unwrap();
    let scale = 8;
    let quiet_zone = scale * 4;
    let gap = scale * 8;
    let first_width = first.width() as u32 * scale;
    let second_width = second.width() as u32 * scale;
    let side_height = (first.width().max(second.width()) as u32) * scale + quiet_zone * 2;
    let image_width = first_width + second_width + quiet_zone * 4 + gap;
    let mut image = GrayImage::from_pixel(image_width, side_height, Luma([255]));
    draw_qr(&mut image, &first, quiet_zone, quiet_zone, scale);
    draw_qr(
        &mut image,
        &second,
        quiet_zone * 3 + first_width + gap,
        quiet_zone,
        scale,
    );
    let path = directory.path("two-authenticators.png");
    DynamicImage::ImageLuma8(image)
        .save_with_format(&path, ImageFormat::Png)
        .unwrap();

    let error = read_totp(&path).unwrap_err();
    assert!(error.to_string().contains("multiple"));
}

#[test]
fn invalid_oversized_and_over_dimension_images_are_rejected() {
    let directory = TestDirectory::new();
    let invalid = directory.path("invalid.png");
    fs::write(&invalid, b"not an image").unwrap();
    assert!(read_totp(&invalid).is_err());

    let oversized = directory.path("oversized.png");
    fs::write(&oversized, vec![0; 8 * 1024 * 1024 + 1]).unwrap();
    assert!(read_totp(&oversized).is_err());

    let too_wide = directory.path("too-wide.png");
    DynamicImage::ImageLuma8(GrayImage::from_pixel(4097, 1, Luma([255])))
        .save_with_format(&too_wide, ImageFormat::Png)
        .unwrap();
    assert!(read_totp(&too_wide).is_err());
}
