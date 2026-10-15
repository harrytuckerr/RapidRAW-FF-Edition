use std::convert::AsRef;
use std::path::Path;

pub const RAW_EXTENSIONS: &[(&str, &str)] = &[
    // Adobe
    ("dng", "Adobe Digital Negative"),
    // Apple
    ("pro", "Apple ProRAW"),
    // Arri
    ("ari", "ARRI Raw"),
    // Canon
    ("crw", "Canon Raw"),
    ("cr2", "Canon Raw 2"),
    ("cr3", "Canon Raw 3"),
    // Casio
    ("bay", "Casio"),
    // Contax
    ("raw", "Contax"),
    // DJI
    // ("dng", "DJI (uses DNG)"), // Covered by Adobe

    // Epson
    ("erf", "Epson Raw"),
    // Fuji
    ("raf", "Fuji Raw"),
    // Hasselblad
    ("3fr", "Hasselblad"),
    ("fff", "Hasselblad"),
    // Imacon / Phase One
    ("iiq", "Imacon/Phase One"),
    // Kodak
    ("kdc", "Kodak"),
    ("k25", "Kodak"),
    ("dcs", "Kodak"),
    ("dcr", "Kodak"),
    // Leaf
    ("mos", "Leaf"),
    // Leica
    ("rwl", "Leica Raw"),
    // ("dng", "Leica (uses DNG)"), // Covered by Adobe

    // Mamiya
    ("mef", "Mamiya"),
    // Minolta
    ("mrw", "Minolta Raw"),
    // Nikon
    ("nef", "Nikon Electronic Format"),
    ("nrw", "Nikon Raw"),
    // Olympus
    ("orf", "Olympus Raw"),
    // Panasonic
    ("rw2", "Panasonic Raw 2"),
    ("raw", "Panasonic Raw"),
    // Pentax
    ("pef", "Pentax Electronic File"),
    ("ptx", "Pentax"),
    // Phase One
    // ("iiq", "Phase One (same as Imacon)"), // Covered by Imacon

    // Ricoh
    // ("dng", "Ricoh (uses DNG)"), // Covered by Adobe

    // Samsung
    ("srw", "Samsung Raw"),
    // Sigma
    ("x3f", "Sigma"),
    // Sony
    ("arw", "Sony Alpha Raw"),
    ("srf", "Sony Raw"),
    ("sr2", "Sony Raw 2"),
]; // Tell me if your's is missing.

pub const NON_RAW_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "tiff", "tif", "webp", "jxl", // Standard formats
    "exr", "hdr", // High Dynamic Range / Wide Gamut
    "tga", "ico", "dds", // Graphics & Icons
    "qoi", "ff", // Simple/Specialist formats
    "pnm", "pbm", "pgm", "ppm", "pam", // Netpbm family
];

pub fn is_raw_file<P: AsRef<Path>>(path: P) -> bool {
    let ext = match path.as_ref().extension().and_then(|s| s.to_str()) {
        Some(e) => e,
        None => return false,
    };

    RAW_EXTENSIONS
        .iter()
        .any(|(raw_ext, _)| raw_ext.eq_ignore_ascii_case(ext))
}

pub fn is_supported_image_file<P: AsRef<Path>>(path: P) -> bool {
    let path = path.as_ref();

    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
    {
        return false;
    }

    let ext = match path.extension().and_then(|s| s.to_str()) {
        Some(e) => e,
        None => return false,
    };

    if RAW_EXTENSIONS
        .iter()
        .any(|(raw_ext, _)| raw_ext.eq_ignore_ascii_case(ext))
    {
        return true;
    }

    NON_RAW_EXTENSIONS
        .iter()
        .any(|non_raw_ext| non_raw_ext.eq_ignore_ascii_case(ext))
}

/// A contradiction between an image's name and its content that can be reported
/// before decoding. Passing the check says nothing about whether the content is
/// a valid image; unknown or truncated data is left to the decoders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageContentIssue {
    Empty,
    RafHasJpegSignature,
}

const JPEG_SIGNATURE: &[u8] = &[0xFF, 0xD8, 0xFF];

pub fn validate_image_content<P: AsRef<Path>>(
    path: P,
    bytes: &[u8],
) -> Result<(), ImageContentIssue> {
    if bytes.is_empty() {
        return Err(ImageContentIssue::Empty);
    }

    // Only RAF is checked: other RAW formats (e.g. Minolta JFIF/EXIF) can
    // legitimately begin with a JPEG signature.
    let is_raf = path
        .as_ref()
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("raf"));

    if is_raf && bytes.starts_with(JPEG_SIGNATURE) {
        return Err(ImageContentIssue::RafHasJpegSignature);
    }

    Ok(())
}

impl ImageContentIssue {
    pub fn open_error_message<P: AsRef<Path>>(&self, path: P) -> String {
        let name = path
            .as_ref()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "image".to_string());

        match self {
            ImageContentIssue::Empty => format!("“{}” is empty and cannot be opened.", name),
            ImageContentIssue::RafHasJpegSignature => format!(
                "“{}” has a JPEG signature, but its name identifies it as a RAF RAW file. Select the original RAW file and try again.",
                name
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JPEG_PREFIX: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F'];

    #[test]
    fn empty_content_is_reported_for_every_name() {
        for name in ["a.RAF", "a.jpg", "a.dng", "no_extension"] {
            assert_eq!(
                validate_image_content(name, &[]),
                Err(ImageContentIssue::Empty),
                "{name}"
            );
        }
    }

    #[test]
    fn raf_name_with_jpeg_signature_is_a_contradiction_in_any_case() {
        for name in ["DSCF5389.RAF", "dscf5389.raf", "DSCF5389.RaF", "/a/b/x.rAf"] {
            assert_eq!(
                validate_image_content(name, JPEG_PREFIX),
                Err(ImageContentIssue::RafHasJpegSignature),
                "{name}"
            );
        }
    }

    #[test]
    fn jpeg_signature_is_not_rejected_for_other_names() {
        // .mrw: rawler decodes Konica Minolta JFIF/EXIF containers, so a JPEG
        // signature is not a contradiction for other RAW extensions.
        for name in [
            "x.jpg",
            "x.JPEG",
            "x.mrw",
            "x.MRW",
            "x.dng",
            "x.raf.jpg",
            "raf",
        ] {
            assert_eq!(validate_image_content(name, JPEG_PREFIX), Ok(()), "{name}");
        }
    }

    #[test]
    fn raf_check_is_not_a_fujifilm_allowlist_or_marker_scan() {
        // Non-JPEG, non-FUJIFILM content (e.g. a DNG/TIFF named .RAF), short or
        // unknown data, and a JPEG embedded at a nonzero offset are left to the
        // decoders rather than relabelled here.
        let mut tiff_with_embedded_jpeg = b"II*\0\x08\0\0\0".to_vec();
        tiff_with_embedded_jpeg.extend_from_slice(JPEG_PREFIX);
        for bytes in [
            &tiff_with_embedded_jpeg[..],
            b"FUJI",
            b"\0",
            &[0xFF, 0xD8],
            b"<html>",
        ] {
            assert_eq!(validate_image_content("x.RAF", bytes), Ok(()), "{bytes:?}");
        }
    }

    #[test]
    fn open_messages_name_only_the_basename_and_observed_condition() {
        let raf = ImageContentIssue::RafHasJpegSignature
            .open_error_message("/storage/emulated/0/Android/media/app/.library/DSCF5389.RAF");
        assert_eq!(
            raf,
            "“DSCF5389.RAF” has a JPEG signature, but its name identifies it as a RAF RAW file. Select the original RAW file and try again."
        );
        assert!(!raf.contains("/storage"));

        assert_eq!(
            ImageContentIssue::Empty.open_error_message("/tmp/x/empty.jpg"),
            "“empty.jpg” is empty and cannot be opened."
        );
    }
}
