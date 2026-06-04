use std::num::ParseFloatError;
use eframe::egui::{self, ColorImage};
use egui::{ImageData, TextureHandle, TextureOptions};
use dicom::{object::{FileDicomObject, InMemDicomObject, Tag, file}};
use dicom_dictionary_std::tags;
use dicom_pixeldata::*;
use image::RgbImage;
use crate::app::GradientEnum;

pub fn imagebuffer_to_color_image_raw(img: &image::ImageBuffer<image::Rgb<u8>, Vec<u8>>) -> ColorImage {
    let (w, h) = img.dimensions();
    let raw = img.as_raw(); // &[u8] in RGBRGB... order
    let mut pixels = Vec::with_capacity(raw.len() / 3);
    for chunk in raw.chunks_exact(3) {
        pixels.push(egui::Color32::from_rgb(chunk[0], chunk[1], chunk[2]));
    }
    ColorImage {
        size: [w as usize, h as usize],
        source_size: egui::Vec2::new(w as f32, h as f32),
        pixels,
    }
}

pub fn convert_to_color_image(image: &image::DynamicImage) -> egui::ColorImage {
    let image_buffer = image.to_rgba8();
    let size = [image_buffer.width() as _, image_buffer.height() as _];
    let pixels = image_buffer.into_raw();
    ColorImage::from_rgba_unmultiplied(size, &pixels)
}

pub fn convert_to_dynamic_image(dcm_image: &DCMImage, options: &ConvertOptions) -> image::DynamicImage {
    let pixeladata = dcm_image.dicom_object.decode_pixel_data().unwrap();
    if options == &ConvertOptions::default() {
        pixeladata.to_dynamic_image(0).unwrap()
    } else {
        pixeladata
            .to_dynamic_image_with_options(0, options)
            .unwrap()
    }
}

pub fn get_image_with_opt_colormap(dcm_image: &DCMImage, wl: &WindowLevel, colormap: &GradientEnum, invert: bool, voi_fn: &VoiLutFunction) -> egui::ColorImage {
    let options = &ConvertOptions::new().with_voi_lut(VoiLutOption::CustomWithFunction(*wl, *voi_fn));
    let pixeldata = dcm_image.dicom_object.decode_pixel_data().unwrap();
    let mut dynamic_image = pixeldata.to_dynamic_image_with_options(0, options).unwrap();
    let colormap = match colormap {
        GradientEnum::Grays => colorous::GREYS,
        GradientEnum::Oranges => colorous::ORANGES,
        GradientEnum::Warm => colorous::WARM,
        GradientEnum::Default => { 
            if invert {
                dynamic_image.invert();
            }
            return convert_to_color_image(&dynamic_image) 
        },
    };

    if !invert {
        dynamic_image.invert();
    }

    let luma_image = dynamic_image.into_luma8();
    let (width, height) = luma_image.dimensions();
    let mut rgb_image = RgbImage::new(width, height);
    

    for (x, y, pixel) in luma_image.enumerate_pixels() {
        let intensity = pixel[0] as usize;
        let color = colormap.eval_rational(intensity, 255);
        rgb_image.put_pixel(x, y, image::Rgb([color.r, color.g, color.b]));
    }

    imagebuffer_to_color_image_raw(&rgb_image)
}

pub struct DCMImage {
    pub dicom_object: FileDicomObject<InMemDicomObject>,
    instance_number: i16,
    series_number: i16,
    // file_name: String,
    // rows: u16,
    // cols: u16,

}

impl DCMImage {
    pub fn read_tag(&self, tag: Tag) -> Option<String> {
        if let Some(t) = self.dicom_object.element_opt(tag).unwrap() {
            return Some(String::from(t.to_str().unwrap()));
        } else {
            None
        }
    }

    pub fn new(bytes: Vec<u8>) -> Self {
        let cursor = std::io::Cursor::new(bytes);
        //let file_name = file_name;
        let dicom_object = file::from_reader(cursor).unwrap();
        let instance_number: i16 =
            if let Some(t) = dicom_object.element_opt(tags::INSTANCE_NUMBER).unwrap() {
                String::from(t.to_str().unwrap()).parse().unwrap_or(0)
            } else {
                0
            };
        let series_number: i16 =
            if let Some(t) = dicom_object.element_opt(tags::SERIES_NUMBER).unwrap() {
                String::from(t.to_str().unwrap()).parse().unwrap_or(-1)
            } else {
                -1
            };
        // let rows = dicom_object.rows().unwrap_or(0);
        // let cols = dicom_object.cols().unwrap_or(0);

        Self {
            dicom_object,
            instance_number,
            series_number,
            // file_name,
            // rows,
            // cols,
        }
    }

    pub fn get_image_with_opt(&self, options: ConvertOptions) -> ColorImage {
        let pixel_data = self.dicom_object.decode_pixel_data().unwrap();
        convert_to_color_image(
            &pixel_data
                .to_dynamic_image_with_options(0, &options)
                .unwrap(),
        )
    }

    pub fn get_image(&self) -> ColorImage {
        let pixel_data = self.dicom_object.decode_pixel_data().unwrap();
        convert_to_color_image(&pixel_data.to_dynamic_image(0).unwrap())
    }
}

pub struct DCMSeries {
    pub series: Vec<DCMImage>,
    pub series_number: i16,
    pub modality: String,
    pub patient_id: String,
    pub default_wl: WindowLevel,
    pub description: String,
    pub texture: Option<TextureHandle>,
}

pub trait WindowLevelParse {
    fn custom_parse(&self) -> Result<f64, ParseFloatError>;
}

impl WindowLevelParse for String {
    fn custom_parse(&self) -> Result<f64, ParseFloatError> {
        let mut chars = self.chars().peekable();
        let mut out = String::new();
        if let Some(&c) = chars.peek() {
            if c == '+' || c == '-' {
                out.push(c);
                chars.next();
            }
        }
        let mut seen_dot = false;
        for c in chars {
            if c.is_ascii_digit() {
                out.push(c);
            } else if c == '.' && !seen_dot {
                seen_dot = true;
                out.push(c);
            } else {
                break;
            }
        }
        out.parse::<f64>()
    }
}

impl DCMSeries {
    pub fn new(img: DCMImage) -> Self {
        let series_number: i16 = img
            .read_tag(tags::SERIES_NUMBER)
            .and_then(|t| t.parse().ok())
            .unwrap_or(-1);
        let modality: String = img.read_tag(tags::MODALITY).unwrap_or_default();
        let patient_id: String = img.read_tag(tags::PATIENT_ID).unwrap_or_default();
        let description: String = img.read_tag(tags::SERIES_DESCRIPTION).unwrap_or_default();
        let ww: f64 = img
            .read_tag(tags::WINDOW_WIDTH)
            .and_then(|t| t.custom_parse().ok())
            .unwrap_or(400.0);
        let wc: f64 = img
            .read_tag(tags::WINDOW_CENTER)
            .and_then(|t|  t.custom_parse().ok())
            .unwrap_or(40.0);
        let default_wl = WindowLevel {
            width: ww,
            center: wc,
        };
        let series: Vec<DCMImage> = vec![img];
        Self {
            series_number,
            modality,
            patient_id,
            description,
            default_wl,
            series,
            texture: None,
        }
    }

    pub fn finalize(&mut self, ctx: &egui::Context) {
        self.series
            .sort_by(|a, b| a.instance_number.cmp(&b.instance_number));
        let i = self.series.len() / 2;
        let pixeldata = &self.series[i].dicom_object.decode_pixel_data().unwrap();
        let dynamic_image = pixeldata.to_dynamic_image(0).unwrap().resize_exact(
            64,
            64,
            image::imageops::FilterType::Triangle,
        );
        let color_image = convert_to_color_image(&dynamic_image);
        self.texture = Some(egui::Context::load_texture(
            ctx,
            String::from("thumbnail"),
            ImageData::from(color_image),
            TextureOptions::default(),
        ));
    }
}

pub fn add_to_series(series_vec: &mut Vec<DCMSeries>, image: DCMImage) {
    if let Some(matching) = series_vec
        .iter_mut()
        .find(|a| a.series_number == image.series_number){
        matching.series.push(image);
    } else {
        series_vec.push(DCMSeries::new(image));
    }
    if series_vec.len() > 1 {
        series_vec.sort_by(|a, b| a.series_number.cmp(&b.series_number));
    }
}
