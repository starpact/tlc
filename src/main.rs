#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod daq;
mod postproc;
mod solve;
mod state;
mod util;
mod video;

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use crossbeam::atomic::AtomicCell;
use eframe::{
    egui::{self, FontData, FontDefinitions, Slider, TextEdit, Ui},
    epaint::{Color32, ColorImage, FontFamily},
    CreationContext,
};
use egui_extras::RetainedImage;
use ndarray::ArcArray2;
use state::StartIndex;
use video::VideoData;

const FRAME_AREA_HEIGHT: usize = 512;
const FRAME_AREA_WIDTH: usize = 640;

fn main() -> Result<(), eframe::Error> {
    video::init();
    util::log::init();

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(1024.0, 768.0)),
        default_theme: eframe::Theme::Light,
        ..Default::default()
    };

    eframe::run_native(
        "TLC Helper",
        options,
        Box::new(move |ctx| Box::new(Tlc::new(ctx))),
    )
}

struct Tlc {
    /// User defined unique name of this experiment setting.
    name: String,

    video: Option<(PathBuf, Promise<anyhow::Result<Arc<VideoData>>>)>,

    daq: Option<(PathBuf, Promise<anyhow::Result<ArcArray2<f64>>>)>,

    frame: (RetainedImage, usize),
    frame_index1: usize,
    serial_num: usize,

    _start_index: Option<StartIndex>,

    green2: Option<Promise<anyhow::Result<ArcArray2<u8>>>>,
}

enum Promise<O> {
    Pending(Arc<AtomicCell<Option<O>>>),
    Ready(O),
}

impl<O: Send + 'static> Promise<O> {
    fn spawn<F>(f: F) -> Self
    where
        F: FnOnce(Arc<AtomicCell<Option<O>>>) + Send + 'static,
    {
        let output = Arc::new(AtomicCell::new(None));
        std::thread::spawn({
            let output = output.clone();
            move || f(output)
        });
        Promise::Pending(output)
    }
}

impl Tlc {
    fn new(ctx: &CreationContext) -> Self {
        let font_data = BTreeMap::from_iter([
            (
                "LXGWWenKaiLite".to_owned(),
                FontData::from_static(include_bytes!("../fonts/LXGWWenKaiLite-Regular.ttf")),
            ),
            (
                "NotoEmoji".to_owned(),
                FontData::from_static(include_bytes!("../fonts/NotoEmoji-Regular.ttf")),
            ),
        ]);
        let families = BTreeMap::from_iter([
            (
                FontFamily::Proportional,
                vec!["LXGWWenKaiLite".to_owned(), "NotoEmoji".to_owned()],
            ),
            (FontFamily::Monospace, Vec::new()),
        ]);

        ctx.egui_ctx.set_fonts(FontDefinitions {
            font_data,
            families,
        });

        Self {
            name: String::new(),
            video: None,
            daq: None,
            frame: (
                RetainedImage::from_color_image(
                    "",
                    ColorImage::new([FRAME_AREA_WIDTH, FRAME_AREA_HEIGHT], Color32::GRAY),
                ),
                0,
            ),
            frame_index1: 0,
            serial_num: 0,
            _start_index: None,
            green2: None,
        }
    }

    fn render_video(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("选择视频文件").clicked() {
                if let Some(video_path) = rfd::FileDialog::new()
                    .add_filter("video", &["avi", "mp4"])
                    .pick_file()
                {
                    self.video = Some((
                        video_path.clone(),
                        Promise::spawn(move |output| {
                            output.store(Some(video::read_video(video_path)))
                        }),
                    ));
                }
            }

            let Some((video_path, promise)) = &mut self.video else { return };
            ui.label(video_path.display().to_string());
            match promise {
                Promise::Pending(output) => match output.take() {
                    Some(ret) => {
                        if let Ok(video_data) = &ret {
                            video_data.decode_one(0, 1); // Trigger decoding first frame.
                            self.serial_num = 2;
                            let green2 = Arc::new(AtomicCell::new(None));
                            self.green2 = Some(Promise::Pending(green2.clone()));
                            let video_data = video_data.clone();
                            std::thread::spawn(move || {
                                green2.store(Some(video_data.decode_range(
                                    0,
                                    2000,
                                    (0, 0, 800, 600),
                                )));
                            });
                        }
                        *promise = Promise::Ready(ret);
                    }
                    None => _ = ui.spinner(),
                },
                Promise::Ready(ret) => match ret {
                    Ok(video_data) => {
                        ui.colored_label(Color32::GREEN, "✔︎");
                        ui.label(format!("帧数: {}", video_data.nframes()));
                        ui.label(format!("帧率: {}", video_data.frame_rate()));
                        let (h, w) = video_data.shape();
                        ui.label(format!("高: {h}"));
                        ui.label(format!("宽: {w}"));
                    }
                    Err(e) => _ = ui.label(e.to_string()),
                },
            }
        });
    }

    fn render_daq(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("选择数采文件").clicked() {
                if let Some(daq_path) = rfd::FileDialog::new()
                    .add_filter("daq", &["lvm", "xlsx"])
                    .pick_file()
                {
                    self.daq = Some((
                        daq_path.clone(),
                        Promise::spawn(move |output| output.store(Some(daq::read_daq(daq_path)))),
                    ));
                }
            }
            let Some((daq_path, promise)) = &mut self.daq else { return };
            ui.label(daq_path.display().to_string());
            match promise {
                Promise::Pending(output) => match output.take() {
                    Some(ret) => *promise = Promise::Ready(ret),
                    None => _ = ui.spinner(),
                },
                Promise::Ready(ret) => match ret {
                    Ok(daq_data) => {
                        ui.colored_label(Color32::GREEN, "✔︎");
                        ui.label(format!("行数: {}", daq_data.nrows()));
                        ui.label(format!("列数: {}", daq_data.ncols()));
                    }
                    Err(e) => _ = ui.label(e.to_string()),
                },
            }
        });
    }

    fn render_frame(&mut self, ui: &mut Ui) {
        self.frame.0.show_size(
            ui,
            egui::vec2(FRAME_AREA_WIDTH as f32, FRAME_AREA_HEIGHT as f32),
        );

        let Some((_, Promise::Ready(Ok(video_data)))) = &self.video else { return };

        if let Some((decoded_frame, serial_num)) = video_data.take_decoded_frame() {
            let (h, w) = video_data.shape();
            if serial_num > self.frame.1 {
                let img = ColorImage::from_rgb([w as usize, h as usize], &decoded_frame);
                self.frame = (RetainedImage::from_color_image("", img), serial_num);
            }
        }

        ui.spacing_mut().slider_width = (video_data.shape().1 / 2 - 100) as f32;
        let old_frame_index = self.frame_index1;
        ui.add(Slider::new(&mut self.frame_index1, 1..=video_data.nframes()).clamp_to_range(true));
        ui.reset_style();

        if old_frame_index != self.frame_index1 {
            video_data.decode_one(self.frame_index1 - 1, self.serial_num);
            self.serial_num += 1;
        }
    }

    fn render_green2(&mut self, ui: &mut Ui) {
        let Some(promise) = &mut self.green2 else { return };
        match promise {
            Promise::Pending(output) => match output.take() {
                Some(ret) => *promise = Promise::Ready(ret),
                None => _ = ui.spinner(),
            },
            Promise::Ready(ret) => match ret {
                Ok(green2) => {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::GREEN, "✔︎");
                        ui.label(format!("行数: {}", green2.nrows()));
                        ui.label(format!("列数: {}", green2.ncols()));
                    });
                }
                Err(e) => _ = ui.label(e.to_string()),
            },
        }
    }
}

impl eframe::App for Tlc {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                let label = ui.label("实验组名称");
                TextEdit::singleline(&mut self.name)
                    .hint_text("必填")
                    .show(ui)
                    .response
                    .labelled_by(label.id);
            });

            self.render_video(ui);
            self.render_daq(ui);
            self.render_frame(ui);
            self.render_green2(ui);
        });
    }
}
