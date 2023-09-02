#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(test, feature(test, array_windows))]
#![allow(clippy::too_many_arguments)]

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use crossbeam::atomic::AtomicCell;
use eframe::{
    egui::{self, FontData, FontDefinitions, TextEdit, Ui},
    epaint::{FontFamily, Rgba},
};
use ndarray::ArcArray2;
use video::VideoData;

mod daq;
mod postproc;
mod solve;
mod state;
mod util;
mod video;

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
        Box::new(move |ctx| {
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

            Box::<Tlc>::default()
        }),
    )
}

#[derive(Default)]
struct Tlc {
    name: String,
    video: Task<PathBuf, Arc<VideoData>>,
    daq: Task<PathBuf, ArcArray2<f64>>,
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
    fn render(
        &mut self,
        ui: &mut Ui,
        render_in_progress: impl FnOnce(&mut Ui, &I),
        render_done: impl FnOnce(&mut Ui, &I, &O),
        render_failed: impl FnOnce(&mut Ui, &I, &anyhow::Error),
    ) -> Self {
        loop {
            match std::mem::take(self) {
                Task::Empty => break Task::Empty,
                Task::InProcess(input, output) => match output.take() {
                    Some(ret) => match ret {
                        Ok(output) => *self = Task::Done(input, output),
                        Err(e) => *self = Task::Failed(input, e),
                    },
                    None => {
                        render_in_progress(ui, &input);
                        break Task::InProcess(input, output);
                    }
                },
                Task::Done(input, output) => {
                    render_done(ui, &input, &output);
                    break Task::Done(input, output);
                }
                Task::Failed(input, e) => {
                    render_failed(ui, &input, &e);
                    break Task::Failed(input, e);
                }
            };
        }
    }
}

impl Tlc {
    fn render_video(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("选择视频").clicked() {
                if let Some(video_path) = rfd::FileDialog::new()
                    .add_filter("video", &["avi", "mp4"])
                    .pick_file()
                {
                    let output = Arc::new(AtomicCell::default());
                    self.video = Task::InProcess(video_path.clone(), output.clone());
                    std::thread::spawn(move || output.store(Some(video::read_video(video_path))));
                }
            }
            self.video = self.video.render(
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
                        ui.colored_label(Rgba::GREEN, "✔︎");
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!("nframes: {}", video_data.nframes()));
                        ui.label(format!("frame_rate: {}", video_data.frame_rate()));
                        let (h, w) = video_data.shape();
                        ui.label(format!("height: {h}"));
                        ui.label(format!("width: {w}"));
                    });
                },
                |ui, video_path, e| {
                    ui.horizontal(|ui| {
                        ui.label(video_path.display().to_string());
                        ui.label(e.to_string());
                    });
                },
            );
        });
    }

    fn render_daq(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("选择数采").clicked() {
                if let Some(daq_path) = rfd::FileDialog::new()
                    .add_filter("daq", &["lvm", "xlsx"])
                    .pick_file()
                {
                    let output = Arc::new(AtomicCell::default());
                    self.daq = Task::InProcess(daq_path.clone(), output.clone());
                    std::thread::spawn(move || output.store(Some(daq::read_daq(daq_path))));
                }
            }
            self.daq = self.daq.render(
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
                        ui.colored_label(Rgba::GREEN, "✔︎");
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!("nrows: {}", daq_data.nrows()));
                        ui.label(format!("ncols: {}", daq_data.ncols()));
                    });
                },
                |ui, daq_path, e| {
                    ui.horizontal(|ui| {
                        ui.label(daq_path.display().to_string());
                        ui.label(e.to_string());
                    });
                },
            );
        });
    }
}

impl eframe::App for Tlc {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                let label = ui.label("Experiment case name");
                TextEdit::singleline(&mut self.name)
                    .hint_text("name required")
                    .show(ui)
                    .response
                    .labelled_by(label.id);
            });

            self.render_video(ui);
            self.render_daq(ui);
        });
    }
}
