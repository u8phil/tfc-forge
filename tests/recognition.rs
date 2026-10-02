//! Behavioral tests for the native Rust anvil screenshot recognizer.
//!
//! These mirror `tests/recognition.test.ts` exactly: the four bundled root PNG
//! fixtures, shifted/padded localization, nearest-neighbour upscaling, and
//! fail-closed handling of blank/tiny/malformed inputs.

use std::path::PathBuf;

use tfc_forge::ForgeRule;
use tfc_forge::recognition::{RecognitionError, RgbaImage, recognize_anvil};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn decode(file: &str) -> RgbaImage {
    let path = root().join(file);
    let image =
        image::open(&path).unwrap_or_else(|error| panic!("open {}: {error}", path.display()));
    RgbaImage::try_from(image.to_rgba8()).expect("convert to RgbaImage")
}

fn rule_tuple(rules: &[ForgeRule]) -> String {
    rules
        .iter()
        .map(|rule| format!("{}:{}", rule.rule_type.as_str(), rule.order.as_str()))
        .collect::<Vec<_>>()
        .join(",")
}

fn pad_image(image: &RgbaImage, left: usize, top: usize, right: usize, bottom: usize) -> RgbaImage {
    let width = image.width + left + right;
    let height = image.height + top + bottom;
    let mut data = vec![0u8; width * height * 4];
    for pixel in data.as_chunks_mut::<4>().0 {
        pixel[0] = 24;
        pixel[1] = 18;
        pixel[2] = 14;
        pixel[3] = 255;
    }
    for (y, src_row) in image.data.chunks_exact(image.width * 4).enumerate() {
        let dst_row = ((y + top) * width + left) * 4;
        data[dst_row..dst_row + src_row.len()].copy_from_slice(src_row);
    }
    RgbaImage::new(width, height, data).expect("padded image")
}

fn upscale_nearest(image: &RgbaImage, factor: usize) -> RgbaImage {
    let width = image.width * factor;
    let height = image.height * factor;
    let mut data = vec![0u8; width * height * 4];
    for y in 0..height {
        let sy = y / factor;
        for x in 0..width {
            let sx = x / factor;
            let si = (sy * image.width + sx) * 4;
            let di = (y * width + x) * 4;
            data[di..di + 4].copy_from_slice(&image.data[si..si + 4]);
        }
    }
    RgbaImage::new(width, height, data).expect("upscaled image")
}

fn assert_fixture(
    file: &str,
    origin: (i32, i32),
    scale: i32,
    start: i32,
    target: i32,
    rules: &str,
) {
    let result = recognize_anvil(&decode(file));
    assert_eq!(result.start, Some(start), "{file} start");
    assert_eq!(result.target, Some(target), "{file} target");
    assert_eq!(rule_tuple(&result.rules), rules, "{file} rules");
    assert!(
        (1..=3).contains(&result.rules.len()),
        "{file} rule count {}",
        result.rules.len()
    );
    let panel = result.panel.unwrap_or_else(|| panic!("{file} panel"));
    assert_eq!((panel.x, panel.y), origin, "{file} origin");
    assert_eq!(panel.scale, scale, "{file} scale");
    assert!(
        result.confidence.start > 0.0,
        "{file} start confidence {}",
        result.confidence.start
    );
    assert!(
        result.confidence.target > 0.0,
        "{file} target confidence {}",
        result.confidence.target
    );
    assert_eq!(result.warnings, Vec::<String>::new(), "{file} warnings");
}

#[test]
fn fixture_1_exact_fields() {
    assert_fixture(
        "1.png",
        (60, 44),
        2,
        0,
        109,
        "hit:last,draw:second_last,bend:third_last",
    );
}

#[test]
fn fixture_2_exact_fields() {
    assert_fixture(
        "2.png",
        (65, 64),
        2,
        0,
        54,
        "punch:last,bend:not_last,draw:not_last",
    );
}

#[test]
fn fixture_3_exact_fields() {
    assert_fixture(
        "3.png",
        (42, 30),
        2,
        0,
        60,
        "punch:last,hit:second_last,upset:third_last",
    );
}

#[test]
fn fixture_4_exact_fields() {
    assert_fixture(
        "4.png",
        (43, 37),
        2,
        0,
        100,
        "punch:last,bend:not_last,draw:not_last",
    );
}

#[test]
fn locates_a_padded_shifted_fixture() {
    let original = decode("1.png");
    let padded = pad_image(&original, 137, 91, 23, 47);
    assert_ne!(padded.width, original.width);
    assert_ne!(padded.height, original.height);

    let result = recognize_anvil(&padded);
    assert_eq!(result.start, Some(0));
    assert_eq!(result.target, Some(109));
    assert_eq!(
        rule_tuple(&result.rules),
        "hit:last,draw:second_last,bend:third_last"
    );
    let panel = result.panel.expect("panel");
    assert_eq!(panel.x, 60 + 137);
    assert_eq!(panel.y, 44 + 91);
    assert_eq!(panel.scale, 2);
}

#[test]
fn infers_larger_scale_after_nearest_neighbour_upscale() {
    let original = decode("1.png");
    let scaled = upscale_nearest(&original, 2);
    let result = recognize_anvil(&scaled);
    assert_eq!(result.start, Some(0));
    assert_eq!(result.target, Some(109));
    assert_eq!(
        rule_tuple(&result.rules),
        "hit:last,draw:second_last,bend:third_last"
    );
    let panel = result.panel.expect("panel");
    assert_eq!(panel.scale, 4);
    assert_eq!(panel.x, 120);
    assert_eq!(panel.y, 88);
}

#[test]
fn fails_closed_on_an_image_with_no_anvil_panel() {
    let width = 200;
    let height = 160;
    let mut data = vec![0u8; width * height * 4];
    for pixel in data.as_chunks_mut::<4>().0 {
        pixel[0] = 10;
        pixel[1] = 10;
        pixel[2] = 12;
        pixel[3] = 255;
    }
    let image = RgbaImage::new(width, height, data).expect("blank image");
    let result = recognize_anvil(&image);
    assert!(result.panel.is_none());
    assert!(result.start.is_none());
    assert!(result.target.is_none());
    assert!(result.rules.is_empty());
    assert!(!result.warnings.is_empty());
}

#[test]
fn does_not_throw_on_a_tiny_image() {
    let image = RgbaImage::new(4, 4, vec![0u8; 4 * 4 * 4]).expect("tiny image");
    let result = recognize_anvil(&image);
    assert!(result.panel.is_none());
    assert!(!result.warnings.is_empty());
}

#[test]
fn rejects_malformed_rgba_buffer_at_construction() {
    // Too short for the declared dimensions.
    let error = RgbaImage::new(4, 4, vec![0u8; 63]).unwrap_err();
    assert!(matches!(error, RecognitionError::InvalidBuffer { .. }));

    // Too long as well.
    let error = RgbaImage::new(4, 4, vec![0u8; 65]).unwrap_err();
    assert!(matches!(error, RecognitionError::InvalidBuffer { .. }));

    // Exactly the right length is accepted.
    assert!(RgbaImage::new(4, 4, vec![0u8; 4 * 4 * 4]).is_ok());
}
