use std::io::Error;
use std::path::PathBuf;
use rsraw::RawImage;
use tiff::TiffError;
use rsraw_sys::ushort;
use tiff::encoder::{colortype, TiffEncoder};
use image::{ImageBuffer, Rgb};

#[derive(Debug)]
pub enum BlendingMode{
    /// Add the pixel values of all images together.
    Additive,
    /// Average the pixel values of all images.
    Average,
    /// Only use the brightest pixel value of all images.
    Bright,
    /// Only use the darkest pixel value of all images.
    Dark,
    /// Prefers the pixel values with the highest deviation from the average.
    PreferChanged
}

#[derive(Debug)]
pub enum BlendingError{
    RsRawError(String),
    CouldntUnpack(String),
    CouldntProcess(String),
    TiffError(TiffError),
    IoError(Error),
    NotEnoughImages,
    InvalidRawBuffer,
}

impl From<Error> for BlendingError{
    fn from(e: Error) -> Self {
        BlendingError::IoError(e)
    }
}

impl From<TiffError> for BlendingError{
    fn from(e: TiffError) -> Self {
        BlendingError::TiffError(e)
    }
}

pub fn raw_pixels(image: &RawImage) -> Result<&[ushort], BlendingError>{
    let data = image.as_ref();

    let raw_width = data.sizes.raw_width as u32;
    let raw_height = data.sizes.raw_height as u32;

    let raw_pixel_count = (raw_width * raw_height) as usize;

    let ptr = data.rawdata.raw_image;
    if ptr.is_null() {
        return Err(BlendingError::InvalidRawBuffer);
    }

    unsafe {
        Ok(std::slice::from_raw_parts(ptr, raw_pixel_count))
    }
}

fn raw_pixels_mut(image: &mut RawImage) -> Result<&mut [ushort], BlendingError> {
    let data = image.as_mut();

    let raw_width = data.sizes.raw_width as u32;
    let raw_height = data.sizes.raw_height as u32;

    let raw_pixel_count = (raw_width * raw_height) as usize;

    let ptr = data.rawdata.raw_image;
    if ptr.is_null() {
        return Err(BlendingError::InvalidRawBuffer);
    }

    unsafe {
        Ok(std::slice::from_raw_parts_mut(ptr, raw_pixel_count))
    }
}

fn blend_raw_images(mut raw_imgs: Vec<RawImage>, mode: BlendingMode) -> Result<RawImage, BlendingError> {
    if raw_imgs.len() < 2 {
        return Err(BlendingError::NotEnoughImages);
    }

    let mut main_image = raw_imgs.pop().unwrap();
    main_image.unpack().map_err(|e| BlendingError::CouldntUnpack(e.to_string()))?;

    let main_raw_pixels = raw_pixels_mut(&mut main_image)?;
    let mut pixels = vec![];

    for img in raw_imgs.iter_mut() {
        img.unpack().map_err(|e| BlendingError::CouldntUnpack(e.to_string()))?;
        pixels.push(raw_pixels(img)?);
    }

    blend_pixels(main_raw_pixels, pixels, &mode);
    update_metadata(&mut main_image, &mode, (raw_imgs.len()+1) as u32);
    Ok(main_image)
}

#[derive(Debug)]
pub struct BlendingResult {
    pub tiff: PathBuf,
    pub preview: Option<PathBuf>,
}

pub fn blend_images(raw_imgs: Vec<RawImage>, mode: BlendingMode, generate_preview: bool) -> Result<BlendingResult, BlendingError>{
    let mut blended_image = blend_raw_images(raw_imgs, mode)?;

    let tiff_processed = blended_image.process::<16>().map_err(|e|BlendingError::CouldntProcess(e.to_string()))?;
    let tiff_path = PathBuf::from(format!("{}.tiff", uuid::Uuid::new_v4()));
    let mut tiff_file = std::fs::File::create(&tiff_path)?;
    let mut encoder = TiffEncoder::new(&mut tiff_file)?;
    encoder.new_image::<colortype::RGB16>(tiff_processed.width(), tiff_processed.height())?.write_data(&tiff_processed)?;

    let mut preview_path = None;
    if generate_preview {
        // For preview, 8-bit is enough
        let preview_processed = blended_image.process::<8>().map_err(|e|BlendingError::CouldntProcess(e.to_string()))?;
        let path = PathBuf::from(format!("{}.jpg", uuid::Uuid::new_v4()));

        let img_buffer: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_raw(preview_processed.width(), preview_processed.height(), preview_processed.to_vec())
            .ok_or_else(|| BlendingError::RsRawError("Failed to create image buffer".to_string()))?;

        img_buffer.save(&path).map_err(|e| BlendingError::RsRawError(e.to_string()))?;
        preview_path = Some(path);
    }

    Ok(BlendingResult {
        tiff: tiff_path,
        preview: preview_path,
    })
}

pub fn generate_preview_jpg(raw_imgs: Vec<RawImage>, mode: BlendingMode) -> Result<PathBuf, BlendingError> {
    let mut blended_image = blend_raw_images(raw_imgs, mode)?;
    
    // For preview, 8-bit is enough
    let processed = blended_image.process::<8>().map_err(|e|BlendingError::CouldntProcess(e.to_string()))?;

    let file_path = PathBuf::from(format!("{}.jpg", uuid::Uuid::new_v4()));
    
    let img_buffer: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_raw(processed.width(), processed.height(), processed.to_vec())
        .ok_or_else(|| BlendingError::RsRawError("Failed to create image buffer".to_string()))?;

    img_buffer.save(&file_path).map_err(|e| BlendingError::RsRawError(e.to_string()))?;

    Ok(file_path)
}

pub fn blend_pixels(main_image_pixels: &mut [ushort], other_pixels: Vec<&[ushort]>, mode: &BlendingMode){
    for i in 0..main_image_pixels.len() {
        let pixel = &mut main_image_pixels[i];
        match mode{
            BlendingMode::Additive => {
                for n in 0..other_pixels.len(){
                    *pixel = pixel.saturating_add(other_pixels[n][i]) as ushort;
                }
            }
            BlendingMode::Average => {
                let mut sum = 0;
                for n in 0..other_pixels.len(){
                    sum += other_pixels[n][i];
                }
                *pixel = (sum / other_pixels.len() as ushort) as ushort;
            }
            BlendingMode::Bright => {
                for n in 0..other_pixels.len(){
                    if other_pixels[n][i] > *pixel {
                        *pixel = other_pixels[n][i];
                    }
                }
            }
            BlendingMode::Dark => {
                for n in 0..other_pixels.len(){
                    if other_pixels[n][i] < *pixel {
                        *pixel = other_pixels[n][i];
                    }
                }
            },
            BlendingMode::PreferChanged => {
                let mut pixel_sum = pixel.clone();
                for n in 0..other_pixels.len(){
                    pixel_sum += other_pixels[n][i];
                }
                let pixel_sum = pixel_sum / other_pixels.len() as ushort;

                let mut biggest_deviation = pixel_sum.abs_diff(*pixel);
                for n in 0..other_pixels.len(){
                    let deviation = pixel_sum.abs_diff(other_pixels[n][i]);
                    if deviation > biggest_deviation{
                        biggest_deviation = deviation;
                        *pixel = other_pixels[n][i];
                    }
                }
            }
        }
    }
}

pub fn update_metadata(image: &mut RawImage, mode: &BlendingMode, num_images: u32){
    let imgdata_mut = image.as_mut();
    // Normalisierung der WB-Multiplikatoren.
    let green_avg = (imgdata_mut.color.cam_mul[1] + imgdata_mut.color.cam_mul[3]) / 2.0;
    if green_avg > 0.0 {
        for i in 0..4 {
            imgdata_mut.color.cam_mul[i] /= green_avg;
        }
    }

    if let BlendingMode::Additive = mode {
        imgdata_mut.color.maximum *= num_images;
        imgdata_mut.color.data_maximum *= num_images;
        imgdata_mut.color.black *= num_images;
    }

    imgdata_mut.params.use_camera_wb = 1;
    imgdata_mut.params.use_camera_matrix = 1;
    imgdata_mut.params.no_auto_bright = 0;
    imgdata_mut.params.output_bps = 16;

    imgdata_mut.params.user_mul = imgdata_mut.color.cam_mul;

    // Wir deaktivieren die automatische Anpassung des Schwarzpegels
    imgdata_mut.params.user_black = imgdata_mut.color.black as i32;
    imgdata_mut.params.user_cblack[0] = imgdata_mut.color.cblack[0] as i32;
    imgdata_mut.params.user_cblack[1] = imgdata_mut.color.cblack[1] as i32;
    imgdata_mut.params.user_cblack[2] = imgdata_mut.color.cblack[2] as i32;
    imgdata_mut.params.user_cblack[3] = imgdata_mut.color.cblack[3] as i32;


    // sRGB Gamma
    imgdata_mut.params.gamm[0] = 1.0 / 2.4;
    imgdata_mut.params.gamm[1] = 12.92;
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Read;
    use rsraw::RawImage;
    use super::*;

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

        let res = blend_images(vec![rawfile1.unwrap(), rawfile2.unwrap(), rawfile3.unwrap()], BlendingMode::Average, false).unwrap();
        println!("{:?}", res);
        assert!(res.tiff.exists());
        assert!(res.preview.is_none());
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

        let res = blend_images(vec![rawfile1.unwrap(), rawfile2.unwrap()], BlendingMode::PreferChanged, true).unwrap();
        println!("TIFF saved to: {:?}", res.tiff);
        println!("Preview saved to: {:?}", res.preview);
        
        assert!(res.tiff.exists());
        assert!(res.preview.is_some());
        let preview_path = res.preview.unwrap();
        assert!(preview_path.exists());
        assert_eq!(preview_path.extension().unwrap(), "jpg");
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

        let res = generate_preview_jpg(vec![rawfile1.unwrap(), rawfile2.unwrap()], BlendingMode::Average).unwrap();
        println!("Preview saved to: {:?}", res);
        assert!(res.exists());
        assert_eq!(res.extension().unwrap(), "jpg");
        std::fs::remove_file(res).unwrap();
    }
}
