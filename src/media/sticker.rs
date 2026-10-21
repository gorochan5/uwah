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

pub const STICKER_THUMBNAIL_SIZE: u32 = 252;
pub const STICKER_TRAY_ICON_SIZE: u32 = 96;
pub const MAX_TRAY_ICON_BYTES: usize = 50 * 1024;

/// Processes an input cover image buffer into a compact 96x96 static WebP tray icon (< 50 KB)
/// required by WhatsApp Android's StickerPackValidator.
pub fn process_sticker_pack_tray_icon_webp(input_bytes: &[u8]) -> Result<Vec<u8>> {
    if input_bytes.is_empty() {
        return Err(anyhow!("Tray icon input image data is empty"));
    }

    // If the input is already a compact static WebP (<= 50 KB), preserve it directly.
    if input_bytes.len() >= 12
        && input_bytes.len() <= MAX_TRAY_ICON_BYTES
        && &input_bytes[0..4] == b"RIFF"
        && &input_bytes[8..12] == b"WEBP"
        && !whatsapp_rust::webp::is_animated(input_bytes)
    {
        return Ok(input_bytes.to_vec());
    }

    let decoded_img = ImageReader::new(Cursor::new(input_bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.decode().ok())
        .or_else(|| {
            let first_frame = extract_first_webp_frame(input_bytes)?;
            ImageReader::new(Cursor::new(first_frame))
                .with_guessed_format()
                .ok()
                .and_then(|r| r.decode().ok())
        })
        .ok_or_else(|| anyhow!("Failed to decode cover image for sticker pack tray icon"))?;

    let (w, h) = decoded_img.dimensions();
    if w == 0 || h == 0 {
        return Err(anyhow!("Invalid cover image dimensions"));
    }

    let resized = decoded_img.resize(
        STICKER_TRAY_ICON_SIZE,
        STICKER_TRAY_ICON_SIZE,
        FilterType::Lanczos3,
    );
    let resized_rgba = resized.to_rgba8();
    let (rw, rh) = resized_rgba.dimensions();

    let mut canvas = RgbaImage::from_pixel(
        STICKER_TRAY_ICON_SIZE,
        STICKER_TRAY_ICON_SIZE,
        Rgba([0, 0, 0, 0]),
    );
    let offset_x = (STICKER_TRAY_ICON_SIZE.saturating_sub(rw)) / 2;
    let offset_y = (STICKER_TRAY_ICON_SIZE.saturating_sub(rh)) / 2;
    image::imageops::overlay(&mut canvas, &resized_rgba, offset_x as i64, offset_y as i64);

    let mut webp_bytes = Vec::new();
    let encoder = WebPEncoder::new_lossless(&mut webp_bytes);
    encoder
        .write_image(
            canvas.as_raw(),
            STICKER_TRAY_ICON_SIZE,
            STICKER_TRAY_ICON_SIZE,
            ColorType::Rgba8.into(),
        )
        .context("Failed to encode 96x96 tray icon as WebP")?;

    Ok(webp_bytes)
}

/// Attempts to extract the first single-frame `VP8 ` or `VP8L` chunk from an animated WebP (`ANMF` chunk)
/// and wrap it in a minimal standalone RIFF WEBP container so standard image decoders can decode it.
fn extract_first_webp_frame(input_bytes: &[u8]) -> Option<Vec<u8>> {
    if input_bytes.len() < 12
        || &input_bytes[0..4] != b"RIFF"
        || &input_bytes[8..12] != b"WEBP"
    {
        return None;
    }

    let mut pos = 12;
    while pos + 8 <= input_bytes.len() {
        let fourcc = &input_bytes[pos..pos + 4];
        let chunk_len = u32::from_le_bytes(input_bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        let payload_start = pos + 8;
        let payload_end = payload_start.checked_add(chunk_len)?;
        if payload_end > input_bytes.len() {
            break;
        }

        if fourcc == b"ANMF" && chunk_len > 16 {
            // Inside ANMF: 16-byte frame header followed by frame subchunks (ALPH, VP8 , VP8L)
            let mut sub_pos = payload_start + 16;
            while sub_pos + 8 <= payload_end {
                let sub_fourcc = &input_bytes[sub_pos..sub_pos + 4];
                let sub_len = u32::from_le_bytes(
                    input_bytes[sub_pos + 4..sub_pos + 8].try_into().ok()?,
                ) as usize;
                let sub_data_start = sub_pos + 8;
                let sub_data_end = sub_data_start.checked_add(sub_len)?;
                if sub_data_end > payload_end {
                    break;
                }

                if sub_fourcc == b"VP8 " || sub_fourcc == b"VP8L" {
                    let padded_sub_len = sub_len + (sub_len & 1);
                    let total_chunk_bytes = 8 + padded_sub_len;
                    let riff_size = (4 + total_chunk_bytes) as u32;

                    let mut out = Vec::with_capacity(12 + total_chunk_bytes);
                    out.extend_from_slice(b"RIFF");
                    out.extend_from_slice(&riff_size.to_le_bytes());
                    out.extend_from_slice(b"WEBP");
                    out.extend_from_slice(sub_fourcc);
                    out.extend_from_slice(&(sub_len as u32).to_le_bytes());
                    out.extend_from_slice(&input_bytes[sub_data_start..sub_data_end]);
                    if sub_len & 1 == 1 {
                        out.push(0);
                    }
                    return Some(out);
                }

                let padded = sub_len + (sub_len & 1);
                sub_pos = sub_data_start.checked_add(padded)?;
            }
        }

        let padded_chunk = chunk_len + (chunk_len & 1);
        pos = payload_start.checked_add(padded_chunk)?;
    }

    None
}

/// Processes an input image buffer into a 252x252 JPEG thumbnail required for WhatsApp Sticker Packs.
/// If the input is an animated WebP whose container cannot be directly decoded, extracts the first
/// frame or falls back to a clean placeholder thumbnail so sticker pack creation never fails.
pub fn process_sticker_pack_thumbnail_jpeg(input_bytes: &[u8]) -> Result<Vec<u8>> {
    if input_bytes.is_empty() {
        return Err(anyhow!("Thumbnail input image data is empty"));
    }

    let decoded_img = ImageReader::new(Cursor::new(input_bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.decode().ok())
        .or_else(|| {
            let first_frame = extract_first_webp_frame(input_bytes)?;
            ImageReader::new(Cursor::new(first_frame))
                .with_guessed_format()
                .ok()
                .and_then(|r| r.decode().ok())
        });

    let mut canvas = RgbaImage::from_pixel(
        STICKER_THUMBNAIL_SIZE,
        STICKER_THUMBNAIL_SIZE,
        Rgba([255, 255, 255, 255]),
    );

    if let Some(dynamic_img) = decoded_img {
        let (w, h) = dynamic_img.dimensions();
        if w > 0 && h > 0 {
            let resized = dynamic_img.resize(
                STICKER_THUMBNAIL_SIZE,
                STICKER_THUMBNAIL_SIZE,
                FilterType::Lanczos3,
            );
            let resized_rgba = resized.to_rgba8();
            let (rw, rh) = resized_rgba.dimensions();
            let offset_x = (STICKER_THUMBNAIL_SIZE.saturating_sub(rw)) / 2;
            let offset_y = (STICKER_THUMBNAIL_SIZE.saturating_sub(rh)) / 2;
            image::imageops::overlay(&mut canvas, &resized_rgba, offset_x as i64, offset_y as i64);
        }
    }

    let rgb_img = DynamicImage::ImageRgba8(canvas).to_rgb8();
    let mut jpeg_bytes = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_bytes, 85);
    encoder
        .write_image(
            rgb_img.as_raw(),
            STICKER_THUMBNAIL_SIZE,
            STICKER_THUMBNAIL_SIZE,
            ColorType::Rgb8.into(),
        )
        .context("Failed to encode sticker pack thumbnail as JPEG")?;

    Ok(jpeg_bytes)
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

    #[test]
    fn test_process_sticker_pack_thumbnail_jpeg_creates_valid_jpeg() {
        let img = DynamicImage::new_rgba8(120, 80);
        let mut png_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
            .unwrap();

        let jpeg_out =
            process_sticker_pack_thumbnail_jpeg(&png_bytes).expect("Failed to process thumbnail");
        assert!(!jpeg_out.is_empty());

        let decoded = image::load_from_memory(&jpeg_out).expect("Failed to decode output JPEG");
        assert_eq!(decoded.width(), STICKER_THUMBNAIL_SIZE);
        assert_eq!(decoded.height(), STICKER_THUMBNAIL_SIZE);
    }
}
