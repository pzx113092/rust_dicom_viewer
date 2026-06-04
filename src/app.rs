#![warn(clippy::all, rust_2018_idioms)]
use crate::dcm::{DCMImage, DCMSeries, add_to_series, get_image_with_opt_colormap};
use dicom_pixeldata::VoiLutFunction;
use egui::load::SizedTexture;
use egui::{Button, Color32, ImageData, ImageSource, Stroke, TextureHandle};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GradientEnum {
    Default,
    Grays,
    Oranges,
    Warm,
}

#[derive(Debug, PartialEq, Eq)]
#[expect(clippy::upper_case_acronyms)]
enum Enum {
    Brain,
    Abdomen,
    Mediastinum,
    Bone,
    Lung,
    MIP,
    Custom,
}

const CT_PRESETS: [(Enum, dicom_pixeldata::WindowLevel); 6] = [
    (
        Enum::Brain,
        dicom_pixeldata::WindowLevel {
            width: 110.0,
            center: 35.0,
        },
    ),
    (
        Enum::Abdomen,
        dicom_pixeldata::WindowLevel {
            width: 320.0,
            center: 50.0,
        },
    ),
    (
        Enum::Mediastinum,
        dicom_pixeldata::WindowLevel {
            width: 400.0,
            center: 80.0,
        },
    ),
    (
        Enum::Bone,
        dicom_pixeldata::WindowLevel {
            width: 2000.0,
            center: 350.0,
        },
    ),
    (
        Enum::Lung,
        dicom_pixeldata::WindowLevel {
            width: 1500.0,
            center: -500.0,
        },
    ),
    (
        Enum::MIP,
        dicom_pixeldata::WindowLevel {
            width: 380.0,
            center: 120.0,
        },
    ),
];

pub struct ViewPort {
    series_i: usize,
    wl: dicom_pixeldata::WindowLevel,
    wl_custom: dicom_pixeldata::WindowLevel,
    tx_map: BTreeMap<
        usize,
        (
            dicom_pixeldata::WindowLevel,
            GradientEnum,
            VoiLutFunction,
            TextureHandle,
            bool,
        ),
    >,
    cursor: usize,
    colormap: GradientEnum,
    uuid: Uuid,
    voi_lut_fn: VoiLutFunction,
    zoom: f32,
    pan: egui::Vec2,
    ct_preset: Enum,
    invert: bool,
}

impl ViewPort {
    fn new(series: &DCMSeries, series_i: usize, ctx: &egui::Context) -> Self {
        let uuid = Uuid::new_v4();
        let wl = series.default_wl;
        let wl_custom = wl;
        let cursor = 0;
        let mut tx_map = BTreeMap::new();
        let voi_lut_fn = VoiLutFunction::Linear;
        for (i, image) in series.series.iter().enumerate() {
            let img = image.get_image();
            let texture = egui::Context::load_texture(
                ctx,
                i.to_string(),
                ImageData::from(img),
                egui::TextureOptions::default(),
            );
            tx_map.insert(i, (wl, GradientEnum::Default, voi_lut_fn, texture, false));
        }

        Self {
            series_i,
            wl,
            wl_custom,
            voi_lut_fn,
            cursor,
            uuid,
            tx_map,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            ct_preset: Enum::Custom,
            invert: false,
            colormap: GradientEnum::Default,
        }
    }
}

pub struct WebApp {
    file_bytes: std::sync::Arc<std::sync::Mutex<Option<Vec<(String, Vec<u8>)>>>>,
    series_vec: Vec<DCMSeries>,
    viewports: Vec<ViewPort>,
    is_loading: std::sync::Arc<std::sync::Mutex<bool>>,
    windows: HashMap<Uuid, bool>,
}

impl Default for WebApp {
    fn default() -> Self {
        Self {
            file_bytes: std::sync::Arc::new(std::sync::Mutex::new(None)),
            series_vec: vec![],
            viewports: vec![],
            windows: HashMap::new(),
            is_loading: std::sync::Arc::new(std::sync::Mutex::new(false)),
        }
    }
}

impl WebApp {
    /// Called once before the first frame.
    pub fn new() -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.

        // if let Some(storage) = cc.storage {
        //     eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        // } else {
        Default::default()
        // }
    }
    pub fn clear_bytes(&mut self) {
        self.file_bytes = std::sync::Arc::new(std::sync::Mutex::new(None));
    }
    pub fn clear_vec(&mut self) {
        self.series_vec = Vec::new();
    }

    fn new_viewport(&mut self, ctx: &egui::Context, series_i: usize) -> Uuid {
        let vp = ViewPort::new(&self.series_vec[series_i], series_i, ctx);
        let uuid = vp.uuid;
        self.viewports.push(vp);
        uuid
    }

    fn remove_viewport(&mut self, uuid: Uuid) {
        let i = self.viewports.iter().position(|x| x.uuid == uuid);
        if let Some(a) = i {
            self.viewports.swap_remove(a);
        }
    }

    fn find_viewport_imm(&self, uuid: &Uuid) -> Option<&ViewPort> {
        let i = self.viewports.iter().position(|x| x.uuid == *uuid);
        if let Some(a) = i {
            Some(&self.viewports[a])
        } else {
            None
        }
    }

    fn find_viewport(&mut self, uuid: &Uuid) -> Option<&mut ViewPort> {
        let i = self.viewports.iter().position(|x| x.uuid == *uuid);
        if let Some(a) = i {
            Some(&mut self.viewports[a])
        } else {
            None
        }
    }
}

impl eframe::App for WebApp {
    // Called by the framework to save state before shutdown.
    // fn save(&mut self, storage: &mut dyn eframe::Storage) {
    //     eframe::set_value(storage, eframe::APP_KEY, self);
    // }
    /// Called each time the UI needs repainting, which may be many times per second.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.set_visuals(egui::Visuals::dark());
        if let Some(downloaded_files) = self.file_bytes.lock().expect("Unable to lock data").take()
        {
            for (_name, bytes) in downloaded_files {
                let img = DCMImage::new(bytes);
                add_to_series(&mut self.series_vec, img);
            }
            for series in &mut self.series_vec {
                series.finalize(ui.ctx());
            }
            *self.is_loading.lock().expect("idk") = false;
        }

        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            ui.separator();

            if ui.button("Reset").clicked() {
                *self = Self::default();
            }

            if self.series_vec.is_empty() && ui.button("Select File").clicked() {
                let p = std::sync::Arc::<std::sync::Mutex<bool>>::clone(&self.is_loading);
                if !*p.lock().expect("Err") {
                    let is_loading_clone = std::sync::Arc::clone(&self.is_loading);
                    let file_bytes_clone = std::sync::Arc::clone(&self.file_bytes);
                    let ctx_clone = ui.ctx().clone();

                    //#[cfg(target_arch = "wasm32")]
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Some(files) = rfd::AsyncFileDialog::new()
                            .add_filter("DICOM", &["dcm"])
                            .pick_files()
                            .await
                        {
                            *is_loading_clone.lock().expect("Err") = true;
                            ctx_clone.request_repaint();
                            let mut raw_files = Vec::new();
                            for file in files {
                                let name = file.file_name();
                                let bytes = file.read().await;
                                raw_files.push((name, bytes));
                            }
                            *file_bytes_clone.lock().expect("Err") = Some(raw_files);
                            ctx_clone.request_repaint();
                        }
                    });
                }
            }

            let is_loading = *self.is_loading.lock().expect("Err");
            if is_loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Reading DICOM files into memory...");
                });
            }

            ui.add_space(10.0);
        });

        egui::Panel::left("left_panel")
            .resizable(false)
            .show_inside(ui, |ui| {
                egui::containers::Frame::group(ui.style())
                    .stroke(Stroke::new(0.0, Color32::BLACK))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let mut j: Option<usize> = None;
                            for (i, series) in self.series_vec.iter().enumerate() {
                                if ui
                                    .add(
                                        Button::image(ImageSource::Texture(
                                            SizedTexture::from_handle(
                                                series
                                                    .texture
                                                    .as_ref()
                                                    .expect("Thumbnail texture load fail"),
                                            ),
                                        ))
                                        .fill(Color32::BLACK),
                                    )
                                    .clicked()
                                {
                                    j = Some(i);
                                }
                                ui.add(egui::Label::new(&series.description).wrap());
                                ui.separator();
                            }
                            if let Some(k) = j {
                                let id = self.new_viewport(ui.ctx(), k);
                                self.windows.insert(id, true);
                            }
                        });
                    });
            });

        egui::CentralPanel::default_margins().show_inside(ui, |ui| {
            let entries: Vec<(Uuid, bool)> = self.windows.iter().map(|(k, v)| (*k, *v)).collect();

            for (id, was_open) in entries {
                let mut open = was_open;

                let vpi = self
                    .find_viewport_imm(&id)
                    .expect("Unable to find viewport");

                let tex = &vpi.tx_map[&vpi.cursor];
                if tex.0 != vpi.wl_custom
                    || tex.1 != vpi.colormap
                    || tex.2 != vpi.voi_lut_fn
                    || tex.4 != vpi.invert
                {
                    let img = get_image_with_opt_colormap(
                        &self.series_vec[vpi.series_i].series[vpi.cursor],
                        &vpi.wl_custom,
                        &vpi.colormap,
                        vpi.invert,
                        &vpi.voi_lut_fn,
                    );

                    let texture = egui::Context::load_texture(
                        ui.ctx(),
                        vpi.cursor.to_string(),
                        ImageData::from(img.clone()),
                        egui::TextureOptions::default(),
                    );
                    let vp = self.find_viewport(&id).expect("Unable to find viewport");
                    vp.tx_map.insert(
                        vp.cursor,
                        (
                            vp.wl_custom,
                            vp.colormap.clone(),
                            vp.voi_lut_fn,
                            texture,
                            vp.invert,
                        ),
                    );
                }

                let vp = self.find_viewport(&id).expect("Unable to find viewport");
                let texture = &vp.tx_map[&vp.cursor].3;
                let image_size = texture.size_vec2();
                let av_s = ui.available_size_before_wrap();
                egui::Window::new(id.to_string())
                    .constrain_to(ui.available_rect_before_wrap())
                    .resizable(true)
                    .min_size(av_s / 1.4)
                    .max_size(av_s * 0.95)
                    .open(&mut open)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Toolbar:");
                            if ui.button("Reset View").clicked() {
                                vp.zoom = 1.0;
                                vp.pan = egui::Vec2::ZERO;
                            }
                        });

                        ui.separator();

                        egui::Panel::left(format!("{}_left", &id))
                            .resizable(false)
                            .max_size(30.0)
                            .show_inside(ui, |ui| {
                                let available_height = ui.available_height() - 10.0;
                                ui.spacing_mut().slider_width = available_height.max(10.0);
                                ui.add(
                                    egui::Slider::new(&mut vp.cursor, vp.tx_map.len() - 1..=0)
                                        .vertical()
                                        .show_value(false),
                                );
                            });

                        egui::Panel::right(format!("{}_right", &id))
                            .resizable(false)
                            .min_size(250.0)
                            .show_inside(ui, |ui| {
                                if ui.button("Reset window").clicked() {
                                    vp.wl_custom = vp.wl;
                                }
                                ui.separator();
                                ui.add(
                                    egui::Slider::new(&mut vp.wl_custom.width, 1.0..=32768.0)
                                        .text("Window"),
                                );
                                ui.add(
                                    egui::Slider::new(&mut vp.wl_custom.center, -32768.0..=32768.0)
                                        .text("Level"),
                                );
                                ui.separator();

                                let before = vp.wl_custom;
                                let mut found = false;

                                for item in CT_PRESETS {
                                    if item.1 == before {
                                        vp.ct_preset = item.0;
                                        found = true;
                                        break;
                                    }
                                }
                                if !found {
                                    vp.ct_preset = Enum::Custom;
                                }
                                egui::ComboBox::from_label("CT window presets")
                                    .selected_text(format!("{:?}", vp.ct_preset))
                                    .show_ui(ui, |ui| {
                                        for item in CT_PRESETS {
                                            ui.selectable_value(
                                                &mut vp.wl_custom,
                                                item.1,
                                                format!("{:?}", item.0),
                                            );
                                        }
                                    });
                                ui.separator();
                                egui::ComboBox::from_label("LUT")
                                    .selected_text(format!("{:?}", &vp.colormap))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut vp.colormap,
                                            GradientEnum::Default,
                                            "Default",
                                        );
                                        ui.selectable_value(
                                            &mut vp.colormap,
                                            GradientEnum::Grays,
                                            "Grays",
                                        );
                                        ui.selectable_value(
                                            &mut vp.colormap,
                                            GradientEnum::Oranges,
                                            "Oranges",
                                        );
                                        ui.selectable_value(
                                            &mut vp.colormap,
                                            GradientEnum::Warm,
                                            "Warm",
                                        );
                                    });
                                ui.separator();
                                egui::ComboBox::from_label("LUT Shape")
                                    .selected_text(format!("{:?}", &vp.voi_lut_fn))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut vp.voi_lut_fn,
                                            VoiLutFunction::Linear,
                                            "Linear (default)",
                                        );
                                        ui.selectable_value(
                                            &mut vp.voi_lut_fn,
                                            VoiLutFunction::LinearExact,
                                            "Linear exact",
                                        );
                                        ui.selectable_value(
                                            &mut vp.voi_lut_fn,
                                            VoiLutFunction::Sigmoid,
                                            "Sigmoid",
                                        );
                                    });
                                ui.separator();
                                ui.checkbox(&mut vp.invert, "Invert");
                            });

                        egui::CentralPanel::default().show_inside(ui, |ui| {
                            egui::Frame::canvas(ui.style()).show(ui, |ui| {
                                let available_size = ui.available_size();
                                let (scene_response, painter) = ui.allocate_painter(
                                    available_size,
                                    egui::Sense::click_and_drag(),
                                );
                                let scene_rect = scene_response.rect;
                                let pointer_pos =
                                    ui.pointer_hover_pos().unwrap_or(egui::Pos2::ZERO);

                                if scene_response.hovered() {
                                    ui.input(|i| {
                                        if i.modifiers.ctrl {
                                            let raw_zoom_delta = i.zoom_delta();
                                            if raw_zoom_delta != 1.0 {
                                                let dampened = 1.0 + (raw_zoom_delta - 1.0) * 0.25;
                                                vp.zoom *= dampened;
                                                vp.zoom = vp.zoom.clamp(0.1, 10.0);
                                            }
                                        }
                                    });
                                }

                                if ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle))
                                    && (scene_response.hovered() || scene_response.dragged())
                                {
                                    vp.pan += ui.input(|i| i.pointer.delta());
                                }
                                let scaled_size = image_size * vp.zoom;

                                let mut min_pan = egui::Vec2::ZERO;
                                let mut max_pan = egui::Vec2::ZERO;

                                if scaled_size.x > scene_rect.width() {
                                    let max_x = (scaled_size.x - scene_rect.width()) / 2.0;
                                    min_pan.x = -max_x;
                                    max_pan.x = max_x;
                                }
                                if scaled_size.y > scene_rect.height() {
                                    let max_y = (scaled_size.y - scene_rect.height()) / 2.0;
                                    min_pan.y = -max_y;
                                    max_pan.y = max_y;
                                }

                                vp.pan = vp.pan.clamp(min_pan, max_pan);

                                let center_offset = (scene_rect.size() - scaled_size) / 2.0;
                                let image_rect = egui::Rect::from_min_size(
                                    scene_rect.min + center_offset + vp.pan,
                                    scaled_size,
                                );

                                let is_hovering_image = image_rect.contains(pointer_pos);

                                if scene_response.hovered() && is_hovering_image {
                                    ui.input(|i| {
                                        if !i.modifiers.ctrl {
                                            for event in &i.events {
                                                if let egui::Event::MouseWheel { delta, .. } = event
                                                {
                                                    let scroll = -delta.y.signum() as i8;
                                                    if scroll > 0 && vp.cursor < vp.tx_map.len() - 1
                                                    {
                                                        vp.cursor += scroll.unsigned_abs() as usize;
                                                    } else if scroll < 0 && vp.cursor > 0 {
                                                        vp.cursor -= scroll.unsigned_abs() as usize;
                                                    }
                                                }
                                            }
                                        }
                                    });
                                }

                                if scene_response.dragged_by(egui::PointerButton::Primary)
                                    && let Some(origin) = ui.input(|i| i.pointer.press_origin())
                                    && image_rect.contains(origin)
                                {
                                    let x = scene_response.drag_delta().x as f64;
                                    if (x > 0.0 && vp.wl_custom.width < 32768.0)
                                        || (x < 0.0 && vp.wl_custom.width > 1.0)
                                    {
                                        vp.wl_custom.width += x;
                                    }

                                    let y = -scene_response.drag_delta().y as f64;
                                    if (y > 0.0 && vp.wl_custom.center < 32768.0)
                                        || (y < 0.0 && vp.wl_custom.center > -32768.0)
                                    {
                                        vp.wl_custom.center += y;
                                    }
                                }

                                painter.image(
                                    texture.id(),
                                    image_rect,
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    egui::Color32::WHITE,
                                );

                                // painter.text(
                                //     scene_rect.left_top() + egui::vec2(10.0, 10.0),
                                //     egui::Align2::LEFT_TOP,
                                //     format!(
                                //         "Zoom: {:.2}x\nScroll Val: {}\nDrag Val: x:{:.1}, y:{:.1}\nWL: ({:.1}, {:.1})",
                                //         vp.zoom, vp.scroll_value, vp.wl_custom.width, vp.wl_custom.center, vp.wl.width, vp.wl.center
                                //     ),
                                //     egui::FontId::proportional(14.0),
                                //     egui::Color32::GREEN,
                                // );
                            });
                        });
                    });

                if !open {
                    self.windows.remove(&id);
                    self.remove_viewport(id);
                }
                if !was_open {
                    self.remove_viewport(id);
                }
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                egui::warn_if_debug_build(ui);
            });
        });
    }
}
