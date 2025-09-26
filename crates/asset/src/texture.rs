//! Texture loading and data structures.
//! E2: Load RGBA8 textures from PNG files.

use std::path::Path;

/// Texture data in CPU-friendly format before GPU upload.
#[derive(Clone, Debug)]
pub struct TextureData {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
}

/// Supported texture formats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextureFormat {
    Rgba8,
}

impl TextureData {
    /// Create a new texture with given dimensions and RGBA8 format.
    pub fn new_rgba8(width: u32, height: u32, data: Vec<u8>) -> Self {
        assert_eq!(
            data.len(),
            (width * height * 4) as usize,
            "Data size doesn't match RGBA8 format"
        );
        Self {
            data,
            width,
            height,
            format: TextureFormat::Rgba8,
        }
    }

    /// Load texture from PNG file.
    pub fn load_png<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();
        log::info!("Loading texture from {:?}", path);

        let img = image::open(path)
            .map_err(|e| anyhow::anyhow!("Failed to open image {:?}: {}", path, e))?;

        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        let data = rgba.into_raw();

        log::info!("Loaded texture {}x{} with {} bytes", width, height, data.len());

        Ok(Self::new_rgba8(width, height, data))
    }

    /// Create a simple test texture (checkerboard pattern).
    pub fn create_test_texture(size: u32) -> Self {
        let mut data = Vec::with_capacity((size * size * 4) as usize);

        for y in 0..size {
            for x in 0..size {
                let checker = ((x / 8) + (y / 8)) % 2;
                if checker == 0 {
                    // White square
                    data.extend_from_slice(&[255, 255, 255, 255]);
                } else {
                    // Gray square
                    data.extend_from_slice(&[128, 128, 128, 255]);
                }
            }
        }

        Self::new_rgba8(size, size, data)
    }

    /// Get the number of bytes per pixel for the format.
    pub fn bytes_per_pixel(&self) -> u32 {
        match self.format {
            TextureFormat::Rgba8 => 4,
        }
    }

    /// Check if the texture data is valid.
    pub fn is_valid(&self) -> bool {
        let expected_size = (self.width * self.height * self.bytes_per_pixel()) as usize;
        self.data.len() == expected_size && self.width > 0 && self.height > 0
    }
}