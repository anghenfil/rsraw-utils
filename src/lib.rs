pub mod blending;

pub use blending::blend_raw_images;

use std::io::Error;
use std::path::Path;
use image::{ImageBuffer, ImageFormat, Rgb};
use rsraw::RawImage;
use tiff::TiffError;
use rsraw_sys::ushort;
use serde::{Deserialize, Serialize};
use tiff::encoder::{colortype, TiffEncoder};

#[derive(Debug)]
pub enum RsRawUtilsError {
    RsRawError(String),
    CouldntUnpack(String),
    CouldntProcess(String),
    TiffError(TiffError),
    IoError(Error),
    NotEnoughImages,
    InvalidRawBuffer,
}

impl std::fmt::Display for RsRawUtilsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RsRawUtilsError::RsRawError(e) => write!(f, "{}", e),
            RsRawUtilsError::CouldntUnpack(e) => write!(f, "{}", e),
            RsRawUtilsError::CouldntProcess(e) => write!(f, "{}", e),
            RsRawUtilsError::TiffError(e) => write!(f, "{}", e),
            RsRawUtilsError::IoError(e) => write!(f, "{}", e),
            RsRawUtilsError::NotEnoughImages => write!(f, "Not enough images to blend"),
            RsRawUtilsError::InvalidRawBuffer => write!(f, "Invalid raw buffer"),
        }
    }
}

impl From<Error> for RsRawUtilsError {
    fn from(e: Error) -> Self {
        RsRawUtilsError::IoError(e)
    }
}

impl From<TiffError> for RsRawUtilsError {
    fn from(e: TiffError) -> Self {
        RsRawUtilsError::TiffError(e)
    }
}

pub fn raw_pixels(image: &RawImage) -> Result<&[ushort], RsRawUtilsError>{
    let data = image.as_ref();

    let raw_width = data.sizes.raw_width as u32;
    let raw_height = data.sizes.raw_height as u32;

    let raw_pixel_count = (raw_width * raw_height) as usize;

    let ptr = data.rawdata.raw_image;
    if ptr.is_null() {
        return Err(RsRawUtilsError::InvalidRawBuffer);
    }

    unsafe {
        Ok(std::slice::from_raw_parts(ptr, raw_pixel_count))
    }
}

fn raw_pixels_mut(image: &mut RawImage) -> Result<&mut [ushort], RsRawUtilsError> {
    let data = image.as_mut();

    let raw_width = data.sizes.raw_width as u32;
    let raw_height = data.sizes.raw_height as u32;

    let raw_pixel_count = (raw_width * raw_height) as usize;

    let ptr = data.rawdata.raw_image;
    if ptr.is_null() {
        return Err(RsRawUtilsError::InvalidRawBuffer);
    }

    unsafe {
        Ok(std::slice::from_raw_parts_mut(ptr, raw_pixel_count))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputFormat{
    JPEG,
    TIFF,
    PNG,
}

pub fn convert_raw(mut raw_image: RawImage, format: OutputFormat, destination: &Path) -> Result<(), RsRawUtilsError>{
    raw_image.unpack().map_err(|e| RsRawUtilsError::CouldntUnpack(e.to_string()))?;
    blending::update_metadata(&mut raw_image, None, 1);

    match format{
        OutputFormat::TIFF => {
            let tiff_processed = raw_image.process::<16>().map_err(|e| RsRawUtilsError::CouldntProcess(e.to_string()))?;
            let mut tiff_file = std::fs::File::create(destination)?;
            let mut encoder = TiffEncoder::new(&mut tiff_file)?;
            encoder.new_image::<colortype::RGB16>(tiff_processed.width(), tiff_processed.height())?.write_data(&tiff_processed)?;
        },
        OutputFormat::JPEG => {
            let processed = raw_image.process::<8>().map_err(|e| RsRawUtilsError::CouldntProcess(e.to_string()))?;

            let img_buffer: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_raw(processed.width(), processed.height(), processed.to_vec())
                .ok_or_else(|| RsRawUtilsError::RsRawError("Failed to create image buffer".to_string()))?;

            img_buffer.save_with_format(destination, ImageFormat::Jpeg).map_err(|e| RsRawUtilsError::RsRawError(e.to_string()))?;
        },
        OutputFormat::PNG => {
            let processed = raw_image.process::<8>().map_err(|e| RsRawUtilsError::CouldntProcess(e.to_string()))?;

            let img_buffer: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_raw(processed.width(), processed.height(), processed.to_vec())
                .ok_or_else(|| RsRawUtilsError::RsRawError("Failed to create image buffer".to_string()))?;

            img_buffer.save_with_format(destination, ImageFormat::Png).map_err(|e| RsRawUtilsError::RsRawError(e.to_string()))?;
        },
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Read;
    use rsraw::RawImage;
    use crate::blending::{BlendingMode, blend_raw_images};
    use crate::{convert_raw, OutputFormat};
    use std::path::Path;

    #[test]
    fn load_raw() {
        let mut raw1 = File::open("test1.ARW").unwrap();
        let mut buf = vec![];
        raw1.read_to_end(&mut buf).unwrap();
        let rawfile1 = RawImage::open(&buf);

        let mut raw2 = File::open("test2.ARW").unwrap();
        let mut buf2 = vec![];
        raw2.read_to_end(&mut buf2).unwrap();
        let rawfile2 = RawImage::open(&buf2);

        let mut raw3 = File::open("test3.ARW").unwrap();
        let mut buf3 = vec![];
        raw3.read_to_end(&mut buf3).unwrap();
        let rawfile3 = RawImage::open(&buf3);

        let res = blend_raw_images(vec![rawfile1.unwrap(), rawfile2.unwrap(), rawfile3.unwrap()], BlendingMode::Average).unwrap();
        let output_path = Path::new("test_output.tiff");
        convert_raw(res, OutputFormat::TIFF, output_path).unwrap();
        assert!(output_path.exists());
        std::fs::remove_file(output_path).unwrap();
    }

    #[test]
    fn test_blend_with_preview() {
        let mut raw1 = File::open("test1.ARW").unwrap();
        let mut buf = vec![];
        raw1.read_to_end(&mut buf).unwrap();
        let rawfile1 = RawImage::open(&buf);

        let mut raw2 = File::open("test2.ARW").unwrap();
        let mut buf2 = vec![];
        raw2.read_to_end(&mut buf2).unwrap();
        let rawfile2 = RawImage::open(&buf2);

        let res = blend_raw_images(vec![rawfile1.unwrap(), rawfile2.unwrap()], BlendingMode::PreferChanged).unwrap();
        let output_path = Path::new("test_output_with_preview.tiff");
        convert_raw(res, OutputFormat::TIFF, output_path).unwrap();
        assert!(output_path.exists());
        std::fs::remove_file(output_path).unwrap();
    }

    #[test]
    fn test_preview_jpg() {
        let mut raw1 = File::open("test1.ARW").unwrap();
        let mut buf = vec![];
        raw1.read_to_end(&mut buf).unwrap();
        let rawfile1 = RawImage::open(&buf);

        let mut raw2 = File::open("test2.ARW").unwrap();
        let mut buf2 = vec![];
        raw2.read_to_end(&mut buf2).unwrap();
        let rawfile2 = RawImage::open(&buf2);

        let res = blend_raw_images(vec![rawfile1.unwrap(), rawfile2.unwrap()], BlendingMode::Average).unwrap();
        let output_path = Path::new("test_preview.jpg");
        convert_raw(res, OutputFormat::JPEG, output_path).unwrap();
        assert!(output_path.exists());
        std::fs::remove_file(output_path).unwrap();
    }

    #[test]
    fn test_convert_raw_to_tiff_and_jpeg_no_blending() {
        // Test TIFF conversion
        let mut raw1 = File::open("test1.ARW").unwrap();
        let mut buf = vec![];
        raw1.read_to_end(&mut buf).unwrap();
        let rawfile1 = RawImage::open(&buf).unwrap();

        let tiff_path = Path::new("test_convert.tiff");
        convert_raw(rawfile1, OutputFormat::TIFF, tiff_path).expect("Failed to convert raw to TIFF");
        assert!(tiff_path.exists());

        // Test JPEG conversion (loading raw again as convert_raw takes ownership)
        let mut raw2 = File::open("test1.ARW").unwrap();
        let mut buf2 = vec![];
        raw2.read_to_end(&mut buf2).unwrap();
        let rawfile2 = RawImage::open(&buf2).unwrap();

        let jpeg_path = Path::new("test_convert.jpg");
        convert_raw(rawfile2, OutputFormat::JPEG, jpeg_path).expect("Failed to convert raw to JPEG");
        assert!(jpeg_path.exists());
    }
}
