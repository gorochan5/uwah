use anyhow::{Context, Result, anyhow};
use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::{ColorType, DynamicImage, GenericImageView, ImageEncoder, ImageReader, Rgba, RgbaImage};
use std::io::Cursor;

pub const STICKER_CANVAS_SIZE: u32 = 512;

/// Processes an input image buffer (PNG, JPEG, GIF, WebP) into a standard WhatsApp sticker WebP buffer.
///
/// Steps:
/// 1. Decodes image bytes.
/// 2. Resizes image preserving aspect ratio to fit inside 512x512 pixels.
/// 3. Places the resized image centered onto a transparent 512x512 canvas.
/// 4. Encodes the canvas into WebP format using `WebPEncoder`.
pub fn process_sticker_image(input_bytes: &[u8]) -> Result<Vec<u8>> {
    if input_bytes.is_empty() {
        return Err(anyhow!("Input image data is empty"));
    }

    // If the input is already a WebP image (e.g. animated or pre-formatted 512x512 WebP sticker),
    // preserve the original bytes directly so animation frames and metadata are not stripped.
    if input_bytes.len() >= 12
        && &input_bytes[0..4] == b"RIFF"
        && &input_bytes[8..12] == b"WEBP"
    {
        return Ok(input_bytes.to_vec());
    }

    // Attempt to guess format and decode image
    let reader = ImageReader::new(Cursor::new(input_bytes))
        .with_guessed_format()
        .context("Failed to determine image format from input data")?;

    let dynamic_img: DynamicImage = reader
        .decode()
        .context("Failed to decode image data into dynamic image")?;

    let (orig_width, orig_height) = dynamic_img.dimensions();
    if orig_width == 0 || orig_height == 0 {
        return Err(anyhow!("Invalid image dimensions (width or height is 0)"));
    }

    // Resize preserving aspect ratio to fit inside STICKER_CANVAS_SIZE x STICKER_CANVAS_SIZE
    let resized_img = dynamic_img.resize(
        STICKER_CANVAS_SIZE,
        STICKER_CANVAS_SIZE,
        FilterType::Lanczos3,
    );
    let resized_rgba = resized_img.to_rgba8();
    let (resized_w, resized_h) = resized_rgba.dimensions();

    // Create transparent 512x512 canvas
    let mut canvas = RgbaImage::from_pixel(
        STICKER_CANVAS_SIZE,
        STICKER_CANVAS_SIZE,
        Rgba([0, 0, 0, 0]),
    );

    // Calculate offsets to center the image on the canvas
    let offset_x = (STICKER_CANVAS_SIZE.saturating_sub(resized_w)) / 2;
    let offset_y = (STICKER_CANVAS_SIZE.saturating_sub(resized_h)) / 2;

    // Overlay resized image onto canvas
    image::imageops::overlay(&mut canvas, &resized_rgba, offset_x as i64, offset_y as i64);

    // Encode to WebP format
    let mut webp_bytes = Vec::new();
    let encoder = WebPEncoder::new_lossless(&mut webp_bytes);
    encoder
        .write_image(
            canvas.as_raw(),
            STICKER_CANVAS_SIZE,
            STICKER_CANVAS_SIZE,
            ColorType::Rgba8.into(),
        )
        .context("Failed to encode canvas as WebP sticker")?;

    Ok(webp_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_sticker_image_creates_512x512_webp() {
        let img = DynamicImage::new_rgb8(100, 200);
        let mut png_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
            .unwrap();

        let webp_out = process_sticker_image(&png_bytes).expect("Failed to process sticker");
        assert!(!webp_out.is_empty());

        let decoded = image::load_from_memory(&webp_out).expect("Failed to decode output WebP");
        assert_eq!(decoded.width(), 512);
        assert_eq!(decoded.height(), 512);
    }
}
