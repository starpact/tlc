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
    egui::{self, FontData, FontDefinitions, TextEdit, Ui},
    epaint::{Color32, FontFamily},
    CreationContext,
};
use egui_extras::RetainedImage;
use ndarray::ArcArray2;
use state::StartIndex;
use video::VideoData;

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

#[derive(Default)]
struct Tlc {
    /// User defined unique name of this experiment setting.
    name: String,
    video: Task<PathBuf, Arc<VideoData>>,
    daq: Task<PathBuf, ArcArray2<f64>>,
    _start_index: Option<StartIndex>,
    green2: Task<(), ArcArray2<u8>>,
}

#[derive(Default)]
enum Task<I, O> {
    #[default]
    Empty,
    InProcess(I, Arc<AtomicCell<Option<anyhow::Result<O>>>>),
    Done(I, O),
    Failed(I, anyhow::Error),
}

impl<I, O> Task<I, O> {
    fn poll(
        &mut self,
        ui: &mut Ui,
        render_in_progress: impl FnOnce(&mut Ui, &I),
        render_done: impl FnOnce(&mut Ui, &I, &O),
        render_failed: impl FnOnce(&mut Ui, &I, &anyhow::Error),
        on_done: impl FnOnce(&I, &O),
    ) {
        *self = match std::mem::take(self) {
            Task::Empty => Task::Empty,
            Task::InProcess(input, output) => match output.take() {
                Some(ret) => match ret {
                    Ok(output) => {
                        render_done(ui, &input, &output);
                        on_done(&input, &output);
                        Task::Done(input, output)
                    }
                    Err(e) => {
                        render_failed(ui, &input, &e);
                        Task::Failed(input, e)
                    }
                },
                None => {
                    render_in_progress(ui, &input);
                    Task::InProcess(input, output)
                }
            },
            Task::Done(input, output) => {
                render_done(ui, &input, &output);
                Task::Done(input, output)
            }
            Task::Failed(input, e) => {
                render_failed(ui, &input, &e);
                Task::Failed(input, e)
            }
        };
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

        Self::default()
    }

    fn render_video(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("选择视频文件").clicked() {
                if let Some(video_path) = rfd::FileDialog::new()
                    .add_filter("video", &["avi", "mp4"])
                    .pick_file()
                {
                    let output = Arc::new(AtomicCell::default());
                    self.video = Task::InProcess(video_path.clone(), output.clone());
                    std::thread::spawn(move || output.store(Some(video::read_video(video_path))));
                }
            }
            self.video.poll(
                ui,
                |ui, video_path| {
                    ui.horizontal(|ui| {
                        ui.label(video_path.display().to_string());
                        ui.spinner();
                    });
                },
                |ui, video_path, video_data| {
                    ui.horizontal(|ui| {
                        ui.label(video_path.display().to_string());
                        ui.colored_label(Color32::GREEN, "✔︎");
                        ui.label(format!("帧数: {}", video_data.nframes()));
                        ui.label(format!("帧率: {}", video_data.frame_rate()));
                        let (h, w) = video_data.shape();
                        ui.label(format!("高: {h}"));
                        ui.label(format!("宽: {w}"));
                    });
                },
                |ui, video_path, e| {
                    ui.horizontal(|ui| {
                        ui.label(video_path.display().to_string());
                        ui.label(e.to_string());
                    });
                },
                |_, video_data| {
                    video_data.decode_one_frame(0, 0); // Trigger render first frame.
                    let green2 = Arc::new(AtomicCell::new(None));
                    self.green2 = Task::InProcess((), green2.clone());
                    let video_data = video_data.clone();
                    std::thread::spawn(move || {
                        green2.store(Some(video_data.decode_range_frames(
                            0,
                            2000,
                            (0, 0, 800, 600),
                        )));
                    });
                },
            );
        });
    }

    fn render_daq(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("选择数采文件").clicked() {
                if let Some(daq_path) = rfd::FileDialog::new()
                    .add_filter("daq", &["lvm", "xlsx"])
                    .pick_file()
                {
                    let output = Arc::new(AtomicCell::default());
                    self.daq = Task::InProcess(daq_path.clone(), output.clone());
                    std::thread::spawn(move || output.store(Some(daq::read_daq(daq_path))));
                }
            }
            self.daq.poll(
                ui,
                |ui, daq_path| {
                    ui.horizontal(|ui| {
                        ui.label(daq_path.display().to_string());
                        ui.spinner();
                    });
                },
                |ui, daq_path, daq_data| {
                    ui.horizontal(|ui| {
                        ui.label(daq_path.display().to_string());
                        ui.colored_label(Color32::GREEN, "✔︎");
                        ui.label(format!("行数: {}", daq_data.nrows()));
                        ui.label(format!("列数: {}", daq_data.ncols()));
                    });
                },
                |ui, daq_path, e| {
                    ui.horizontal(|ui| {
                        ui.label(daq_path.display().to_string());
                        ui.label(e.to_string());
                    });
                },
                |_, _| {},
            );
        });
    }

    fn render_green2(&mut self, ui: &mut Ui) {
        self.green2.poll(
            ui,
            |ui, _| _ = ui.spinner(),
            |ui, _, green2| {
                ui.horizontal(|ui| {
                    ui.colored_label(Color32::GREEN, "✔︎");
                    ui.label(format!("行数: {}", green2.nrows()));
                    ui.label(format!("列数: {}", green2.ncols()));
                });
            },
            |ui, _, e| {
                ui.horizontal(|ui| {
                    ui.label(e.to_string());
                });
            },
            |_, _| {},
        );
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

            if let Task::Done(_, video_data) = &self.video {
                if let Some((decoded_frame, _)) = &*video_data.decoded_frame() {
                    let image = RetainedImage::from_image_bytes("", decoded_frame).unwrap();
                    let (h, w) = video_data.shape();
                    image.show_size(ui, egui::vec2((w / 4) as f32, (h / 4) as f32));
                }
            }

            self.render_green2(ui);
        });
    }
}
