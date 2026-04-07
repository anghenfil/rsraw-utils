use rsraw::RawImage;
use rsraw_sys::ushort;
use serde::{Deserialize, Serialize};
use crate::{raw_pixels, raw_pixels_mut, RsRawUtilsError};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub fn blend_raw_images(mut raw_imgs: Vec<RawImage>, mode: BlendingMode) -> Result<RawImage, RsRawUtilsError> {
    if raw_imgs.len() < 2 {
        return Err(RsRawUtilsError::NotEnoughImages);
    }

    let mut main_image = raw_imgs.pop().unwrap();

    let main_raw_pixels = raw_pixels_mut(&mut main_image)?;
    let mut pixels = vec![];

    for img in raw_imgs.iter_mut() {
        pixels.push(raw_pixels(img)?);
    }

    blend_pixels(main_raw_pixels, pixels, &mode);
    update_metadata(&mut main_image, Some(&mode), (raw_imgs.len()+1) as u32);
    Ok(main_image)
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
                let mut sum = *pixel as u32;
                for n in 0..other_pixels.len(){
                    sum += other_pixels[n][i] as u32;
                }
                *pixel = (sum / (other_pixels.len() + 1) as u32) as ushort;
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
                let mut pixel_sum = *pixel as u32;
                for n in 0..other_pixels.len(){
                    pixel_sum += other_pixels[n][i] as u32;
                }
                let avg = (pixel_sum / (other_pixels.len() + 1) as u32) as ushort;

                let mut biggest_deviation = avg.abs_diff(*pixel);
                for n in 0..other_pixels.len(){
                    let deviation = avg.abs_diff(other_pixels[n][i]);
                    if deviation > biggest_deviation{
                        biggest_deviation = deviation;
                        *pixel = other_pixels[n][i];
                    }
                }
            }
        }
    }
}

pub fn update_metadata(image: &mut RawImage, mode: Option<&BlendingMode>, num_images: u32){
    let imgdata_mut = image.as_mut();
    // Normalisierung der WB-Multiplikatoren.
    let green_avg = (imgdata_mut.color.cam_mul[1] + imgdata_mut.color.cam_mul[3]) / 2.0;
    if green_avg > 0.0 {
        for i in 0..4 {
            imgdata_mut.color.cam_mul[i] /= green_avg;
        }
    }

    if let Some(BlendingMode::Additive) = mode {
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