#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(test, feature(test, array_windows))]
#![allow(clippy::too_many_arguments)]

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use crossbeam::atomic::AtomicCell;
use eframe::{
    egui::{self, FontData, FontDefinitions, TextEdit, Ui},
    epaint::{Color32, FontFamily},
    CreationContext,
};
use ndarray::ArcArray2;
use state::StartIndex;
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
        Box::new(move |ctx| Box::new(Tlc::new(ctx))),
    )
}

#[derive(Default)]
struct Tlc {
    name: String,
    video: Task<PathBuf, Arc<VideoData>>,
    daq: Task<PathBuf, ArcArray2<f64>>,
    _start_index: Option<StartIndex>,
    _green2: Task<(), ArcArray2<u8>>,
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
    ) -> bool {
        let mut changed_to_done = false;

        *self = match std::mem::take(self) {
            Task::Empty => Task::Empty,
            Task::InProcess(input, output) => match output.take() {
                Some(ret) => match ret {
                    Ok(output) => {
                        changed_to_done = true;
                        render_done(ui, &input, &output);
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

        changed_to_done
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

    fn render_video(&mut self, ui: &mut Ui) -> bool {
        let mut changed_to_done = false;

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
            changed_to_done = self.video.poll(
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
                    });
                    ui.horizontal(|ui| {
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
            );
        });

        changed_to_done
    }

    fn render_daq(&mut self, ui: &mut Ui) -> bool {
        let mut changed_to_done = false;

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
            changed_to_done = self.daq.poll(
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
                    });
                    ui.horizontal(|ui| {
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
            );
        });

        changed_to_done
    }
}

impl eframe::App for Tlc {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut need_reconcile = false;

            ui.horizontal(|ui| {
                let label = ui.label("实验组名称");
                TextEdit::singleline(&mut self.name)
                    .hint_text("必填")
                    .show(ui)
                    .response
                    .labelled_by(label.id);
            });

            need_reconcile |= self.render_video(ui);
            need_reconcile |= self.render_daq(ui);

            if need_reconcile {
                println!("try reconcile");
            }
        });
    }
}
