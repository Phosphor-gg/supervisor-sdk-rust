//! Client-side image preprocessing for moderation requests.
//!
//! Images sent for moderation do not need to be full resolution: downscaling
//! and re-encoding before upload cuts bandwidth and latency without affecting
//! moderation quality. This module is enabled by the `image-prep` cargo
//! feature (on by default); when the feature is disabled, [`prepare_image`]
//! is a no-op that returns its input unchanged.

#[cfg(feature = "image-prep")]
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
#[cfg(feature = "image-prep")]
use base64::Engine;

/// Longest allowed side of an uploaded image, in pixels.
#[cfg(feature = "image-prep")]
const MAX_DIMENSION: u32 = 1280;

/// JPEG quality used when re-encoding.
#[cfg(feature = "image-prep")]
const JPEG_QUALITY: u8 = 85;

/// Preprocess a base64-encoded image before sending it for moderation.
///
/// Strips an optional `data:...;base64,` prefix, downscales the image so its
/// longest side is at most 1280 pixels, flattens any transparency onto a
/// white background, and re-encodes as JPEG (quality 85). The result is
/// always raw standard base64. If the input cannot be decoded, or if
/// re-encoding would not make it smaller, the input is returned unchanged.
///
/// This is called automatically by [`SupervisorClient::moderate`],
/// [`SupervisorClient::moderate_batch`] and [`PlatformClient::moderate`];
/// it is exposed so images can also be prepared ahead of time (e.g. before
/// caching or queueing them).
///
/// When the `image-prep` feature is disabled this is a no-op that returns
/// the input unchanged.
///
/// [`SupervisorClient::moderate`]: crate::SupervisorClient::moderate
/// [`SupervisorClient::moderate_batch`]: crate::SupervisorClient::moderate_batch
/// [`PlatformClient::moderate`]: crate::PlatformClient::moderate
pub fn prepare_image(image_b64: &str) -> String {
    #[cfg(feature = "image-prep")]
    {
        prepare(image_b64).unwrap_or_else(|| image_b64.to_string())
    }
    #[cfg(not(feature = "image-prep"))]
    {
        image_b64.to_string()
    }
}

/// Strip a `data:<mediatype>;base64,` prefix, if present.
#[cfg(feature = "image-prep")]
fn strip_data_url_prefix(input: &str) -> &str {
    if let Some(rest) = input.strip_prefix("data:") {
        if let Some(idx) = rest.find(";base64,") {
            return &rest[idx + ";base64,".len()..];
        }
    }
    input
}

/// Core preprocessing pipeline. Returns `None` on any failure, in which case
/// the caller falls back to the unmodified input.
#[cfg(feature = "image-prep")]
fn prepare(image_b64: &str) -> Option<String> {
    let b64 = strip_data_url_prefix(image_b64).trim();
    let bytes = STANDARD
        .decode(b64)
        .or_else(|_| STANDARD_NO_PAD.decode(b64))
        .ok()?;

    // Guard against decompression bombs before decoding.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(1 << 30); // 1 GiB: enough for a 16384x16384 RGBA frame

    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?;
    reader.limits(limits);
    // Animated formats (GIF/WebP) decode to their first frame here.
    let img = reader.decode().ok()?;

    let needs_resize = img.width().max(img.height()) > MAX_DIMENSION;
    let img = if needs_resize {
        // `resize` fits within the bounds while preserving aspect ratio.
        img.resize(MAX_DIMENSION, MAX_DIMENSION, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    // JPEG has no alpha channel, so flatten transparency onto white rather
    // than letting the encoder drop it.
    let rgb = if img.color().has_alpha() {
        flatten_onto_white(&img.to_rgba8())
    } else {
        img.to_rgb8()
    };

    let mut buf = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, JPEG_QUALITY);
    rgb.write_with_encoder(encoder).ok()?;

    // If nothing was downscaled and the re-encode didn't shrink the payload,
    // keep the original bytes.
    if !needs_resize && buf.len() >= bytes.len() {
        return Some(STANDARD.encode(&bytes));
    }

    Some(STANDARD.encode(&buf))
}

/// Composite an RGBA image over a white background, producing opaque RGB.
#[cfg(feature = "image-prep")]
fn flatten_onto_white(rgba: &image::RgbaImage) -> image::RgbImage {
    let mut out = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = pixel[3] as u32;
        let blend = |c: u8| ((c as u32 * alpha + 255 * (255 - alpha)) / 255) as u8;
        out.put_pixel(
            x,
            y,
            image::Rgb([blend(pixel[0]), blend(pixel[1]), blend(pixel[2])]),
        );
    }
    out
}

#[cfg(all(test, feature = "image-prep"))]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
    use std::io::Cursor;

    fn to_png_b64(img: &DynamicImage) -> String {
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .expect("PNG encoding failed");
        STANDARD.encode(&buf)
    }

    fn decode_output(b64: &str) -> (Vec<u8>, DynamicImage) {
        let bytes = STANDARD.decode(b64).expect("output is not standard base64");
        let img = image::ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .expect("format guess failed")
            .decode()
            .expect("output did not decode");
        (bytes, img)
    }

    #[test]
    fn downscales_large_rgb_to_jpeg() {
        let src = RgbImage::from_fn(3000, 2000, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });
        let input = to_png_b64(&DynamicImage::ImageRgb8(src));

        let output = prepare_image(&input);
        let (bytes, img) = decode_output(&output);

        assert_eq!(
            image::guess_format(&bytes).expect("no format guessed"),
            ImageFormat::Jpeg
        );
        assert_eq!(img.width().max(img.height()), 1280);
        // Aspect ratio preserved: 3000x2000 -> 1280 x ~853.
        let expected_h = (2000.0 * 1280.0 / 3000.0f64).round() as i64;
        assert!((img.height() as i64 - expected_h).abs() <= 1);
    }

    #[test]
    fn flattens_transparency_onto_white() {
        // Fully transparent red: after flattening every pixel should be white.
        let src = RgbaImage::from_pixel(4000, 1000, Rgba([255, 0, 0, 0]));
        let input = to_png_b64(&DynamicImage::ImageRgba8(src));

        let output = prepare_image(&input);
        let (bytes, img) = decode_output(&output);

        assert_eq!(
            image::guess_format(&bytes).expect("no format guessed"),
            ImageFormat::Jpeg
        );
        assert_eq!(img.width().max(img.height()), 1280);
        // Aspect ratio preserved: 4000x1000 -> 1280 x ~320.
        let expected_h = (1000.0 * 1280.0 / 4000.0f64).round() as i64;
        assert!((img.height() as i64 - expected_h).abs() <= 1);

        let rgba = img.to_rgba8();
        for pixel in rgba.pixels() {
            assert_eq!(pixel[3], 255, "output must be fully opaque");
            // Allow slack for JPEG compression artifacts.
            assert!(
                pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240,
                "transparent pixels should flatten to white, got {:?}",
                pixel
            );
        }
    }

    #[test]
    fn small_image_keeps_dimensions() {
        let src = RgbImage::from_fn(64, 64, |x, y| Rgb([x as u8 * 4, y as u8 * 4, 128]));
        let input = to_png_b64(&DynamicImage::ImageRgb8(src));

        let output = prepare_image(&input);
        let (_, img) = decode_output(&output);

        assert_eq!((img.width(), img.height()), (64, 64));
    }

    #[test]
    fn invalid_base64_passes_through() {
        assert_eq!(prepare_image("%%%"), "%%%");
    }

    #[test]
    fn non_image_bytes_pass_through() {
        let input = STANDARD.encode(b"hello world");
        assert_eq!(prepare_image(&input), input);
    }
}
