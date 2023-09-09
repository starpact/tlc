#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod daq;
mod postproc;
mod solve;
mod state;
mod util;
mod video;

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use crossbeam::atomic::AtomicCell;
use daq::DaqData;
use eframe::{
    egui::{
        self, Button, CentralPanel, DragValue, FontData, FontDefinitions, Grid, ScrollArea, Slider,
        TextEdit, Ui,
    },
    epaint::{Color32, ColorImage, FontFamily},
    CreationContext,
};
use egui_extras::RetainedImage;
use ndarray::ArcArray2;

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

    video: Option<(PathBuf, Promise<anyhow::Result<VideoData>>)>,

    daq: Option<(PathBuf, Promise<anyhow::Result<DaqData>>)>,

    frame: (RetainedImage, usize),
    frame_index: usize,
    serial_num: usize,

    row_index: usize,

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
            frame_index: 0,
            serial_num: 0,
            row_index: 0,
            green2: None,
        }
    }

    fn render_video_selector(&mut self, ui: &mut Ui) {
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
                            self.frame_index = 0;
                            self.serial_num += 1;
                            video_data.decode_one(0, self.serial_num); // Trigger decoding first frame.

                            let green2 = Arc::new(AtomicCell::new(None));
                            self.green2 = Some(Promise::Pending(green2.clone()));
                            let video_data = video_data.clone();
                            std::thread::spawn(move || {
                                green2.store(Some(video_data.decode_range_area(
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

    fn render_daq_selector(&mut self, ui: &mut Ui) {
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
                        ui.label(format!("行数: {}", daq_data.data().nrows()));
                        ui.label(format!("列数: {}", daq_data.data().ncols()));
                    }
                    Err(e) => _ = ui.label(e.to_string()),
                },
            }
        });
    }

    fn render_video_frame(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            self.frame.0.show_size(
                ui,
                egui::vec2(FRAME_AREA_WIDTH as f32, FRAME_AREA_HEIGHT as f32),
            );

            let Some((_, Promise::Ready(Ok(video_data)))) = &self.video else {
                return;
            };

            if let Some((decoded_frame, serial_num)) = video_data.take_decoded_frame() {
                let (h, w) = video_data.shape();
                let current_frame = self.frame.1;
                tracing::debug!(serial_num, current_frame);
                if serial_num > self.frame.1 {
                    let img = ColorImage::from_rgb([w as usize, h as usize], &decoded_frame);
                    self.frame = (RetainedImage::from_color_image("", img), serial_num);
                }
            }

            ui.scope(|ui| {
                ui.spacing_mut().slider_width = (video_data.shape().1 / 2 - 50) as f32;
                let mut frame_index1 = self.frame_index + 1;
                ui.add(
                    Slider::new(&mut frame_index1, 1..=video_data.nframes()).clamp_to_range(true),
                );
                if frame_index1 != self.frame_index + 1 {
                    self.frame_index = frame_index1 - 1;
                    self.serial_num += 1;
                    video_data.decode_one(self.frame_index, self.serial_num);
                }
            });
        });
    }

    fn render_daq_table(&mut self, ui: &mut Ui) {
        const CELL_WIDTH: f32 = 60.0;
        let Some((_, Promise::Ready(Ok(daq_data)))) = &mut self.daq else { return };
        ui.vertical(|ui| {
            Grid::new("daq_table")
                .min_col_width(CELL_WIDTH)
                .max_col_width(CELL_WIDTH)
                .show(ui, |ui| {
                    ui.label("");
                    assert_eq!(daq_data.data().ncols(), daq_data.thermocouples_mut().len());
                    for (i, tc) in daq_data.thermocouples_mut().iter_mut().enumerate() {
                        ui.vertical(|ui| match tc {
                            Some((y, x)) => {
                                let mut is_tc = true;
                                ui.checkbox(&mut is_tc, (i + 1).to_string());
                                if is_tc {
                                    ui.horizontal(|ui| {
                                        ui.label("y");
                                        ui.add(DragValue::new(y).speed(1.0));
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("x");
                                        ui.add(DragValue::new(x));
                                    });
                                } else {
                                    *tc = None;
                                }
                            }
                            None => {
                                let mut is_tc = false;
                                ui.checkbox(&mut is_tc, (i + 1).to_string());
                                if is_tc {
                                    *tc = Some((0, 0));
                                }
                            }
                        });
                    }
                    ui.end_row();
                });

            ScrollArea::both()
                .min_scrolled_height(200.0)
                .show(ui, |ui| {
                    let current_row_index = self.row_index;
                    Grid::new("daq_table")
                        .min_col_width(CELL_WIDTH)
                        .max_col_width(CELL_WIDTH)
                        .with_row_color(move |i, _| {
                            (i == current_row_index).then_some(Color32::LIGHT_RED)
                        })
                        .show(ui, |ui| {
                            for (i, row) in daq_data.data().rows().into_iter().enumerate() {
                                if ui
                                    .add(
                                        Button::new((i + 1).to_string())
                                            .min_size(egui::vec2(CELL_WIDTH, 0.0)),
                                    )
                                    .clicked()
                                {
                                    self.row_index = i;
                                }

                                for v in row {
                                    ui.label(format!("{v:.2}"));
                                }
                                ui.end_row();
                            }
                        });
                });
        });
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
        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::both().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.label("实验组名称");
                    TextEdit::singleline(&mut self.name)
                        .hint_text("必填")
                        .show(ui)
                        .response
                        .labelled_by(label.id);
                });

                self.render_video_selector(ui);
                self.render_daq_selector(ui);
                self.render_video_frame(ui);
                self.render_green2(ui);
                self.render_daq_table(ui);
            });
        });
    }
}
