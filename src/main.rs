#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod daq;
mod postproc;
mod solve;
mod util;
mod video;

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use crossbeam::atomic::AtomicCell;
use daq::DaqData;
use eframe::{
    egui::{
        self, Button, CentralPanel, ComboBox, DragValue, FontData, FontDefinitions, RichText,
        ScrollArea, Slider, TextEdit, Ui,
    },
    epaint::{Color32, ColorImage, FontFamily},
    CreationContext,
};
use egui_extras::{Column, RetainedImage, TableBuilder};
use ndarray::ArcArray2;

use video::{filter_detect_peak, filter_point, FilterMethod, VideoData};

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

    /// Video data.
    video: Option<(PathBuf, Promise<anyhow::Result<VideoData>>)>,

    /// DAQ data.
    daq: Option<(PathBuf, Promise<anyhow::Result<DaqData>>)>,

    /// Video frame.
    frame: (RetainedImage, usize),
    frame_index: usize,
    serial_num: usize,

    /// DAQ table.
    row_index: usize,

    /// Synchronization.
    /// Start frame of video and start row of DAQ data involved in the calculation,
    /// updated simultaneously.
    start_index: Option<StartIndex>,

    area: Option<(u32, u32, u32, u32)>,

    /// Green2 data.
    green2: Option<Promise<anyhow::Result<ArcArray2<u8>>>>,

    /// Filter and peak detection.
    filter_method: FilterMethod,
    #[allow(clippy::type_complexity)]
    point_green_history: Option<((u32, u32), Promise<anyhow::Result<Vec<u8>>>)>,
    gmax_frame_indexes: Option<Promise<Arc<[usize]>>>,
}

enum Promise<O> {
    Pending(Arc<AtomicCell<Option<O>>>),
    Ready(O),
}

impl<O: Send + 'static> Promise<O> {
    fn spawn<F>(f: F) -> Self
    where
        F: FnOnce() -> O + Send + 'static,
    {
        let output = Arc::new(AtomicCell::new(None));
        let promise = Promise::Pending(output.clone());
        std::thread::spawn(move || output.store(Some(f())));
        promise
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct StartIndex {
    pub start_frame: usize,
    pub start_row: usize,
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
            start_index: None,
            area: Some((0, 0, 800, 600)),
            green2: None,
            filter_method: FilterMethod::No,
            point_green_history: None,
            gmax_frame_indexes: None,
        }
    }

    fn render_experiment_name(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let label = ui.label("实验组名称");
            TextEdit::singleline(&mut self.name)
                .hint_text("必填")
                .show(ui)
                .response
                .labelled_by(label.id);
        });
    }

    fn render_video_selector(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.heading("视频");

            if ui.button("选择视频文件").clicked() {
                if let Some(video_path) = rfd::FileDialog::new()
                    .add_filter("video", &["avi", "mp4"])
                    .pick_file()
                {
                    self.video = Some((
                        video_path.clone(),
                        Promise::spawn(move || video::read_video(video_path)),
                    ));
                }
            }
            if let Some((video_path, _)) = &mut self.video {
                ui.label(video_path.display().to_string());
            }

            let Some((_, promise)) = &mut self.video else { return };
            match promise {
                Promise::Pending(output) => match output.take() {
                    Some(ret) => {
                        if let Ok(video_data) = &ret {
                            self.frame_index = 0;
                            self.serial_num += 1;
                            video_data.decode_one(0, self.serial_num); // Trigger decoding first frame.
                        }
                        *promise = Promise::Ready(ret);
                    }
                    None => _ = ui.spinner(),
                },
                Promise::Ready(ret) => match ret {
                    Ok(video_data) => {
                        ui.horizontal(|ui| {
                            ui.colored_label(Color32::GREEN, "✔︎");
                            ui.label(format!("帧数: {}", video_data.nframes()));
                            ui.label(format!("帧率: {}", video_data.frame_rate()));
                            let (h, w) = video_data.shape();
                            ui.label(format!("高: {h}"));
                            ui.label(format!("宽: {w}"));
                        });
                    }
                    Err(e) => _ = ui.label(e.to_string()),
                },
            }
        });
    }

    fn render_daq_selector(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.heading("数采");

            if ui.button("选择数采文件").clicked() {
                if let Some(daq_path) = rfd::FileDialog::new()
                    .add_filter("daq", &["lvm", "xlsx"])
                    .pick_file()
                {
                    self.daq = Some((
                        daq_path.clone(),
                        Promise::spawn(move || daq::read_daq(daq_path)),
                    ));
                }
            }
            if let Some((daq_path, _)) = &mut self.daq {
                ui.label(daq_path.display().to_string());
            }

            let Some((_, promise)) = &mut self.daq else { return };
            match promise {
                Promise::Pending(output) => match output.take() {
                    Some(ret) => *promise = Promise::Ready(ret),
                    None => _ = ui.spinner(),
                },
                Promise::Ready(ret) => match ret {
                    Ok(daq_data) => {
                        ui.horizontal(|ui| {
                            ui.colored_label(Color32::GREEN, "✔︎");
                            ui.label(format!("行数: {}", daq_data.data().nrows()));
                            ui.label(format!("列数: {}", daq_data.data().ncols()));
                        });
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

            let Some((_, Promise::Ready(Ok(video_data)))) = &self.video else { return };

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
                let slider = Slider::new(&mut self.frame_index, 0..=video_data.nframes() - 1)
                    .clamp_to_range(true);
                if ui.add(slider).changed() {
                    self.serial_num += 1;
                    video_data.decode_one(self.frame_index, self.serial_num);
                };
            });
        });
    }

    fn render_daq_table(&mut self, ui: &mut Ui) {
        const CELL_WIDTH: f32 = 60.0;
        let Some((_, Promise::Ready(Ok(daq_data)))) = &mut self.daq else { return };

        let mut builder = TableBuilder::new(ui);
        builder = builder.column(Column::auto());
        for _ in 0..daq_data.data().ncols() {
            builder = builder.column(Column::auto().at_least(50.0));
        }
        builder
            .header(20.0, |mut header| {
                header.col(|ui| _ = ui.label(""));
                assert_eq!(daq_data.data().ncols(), daq_data.thermocouples_mut().len());
                for (i, tc) in daq_data.thermocouples_mut().iter_mut().enumerate() {
                    header.col(|ui| {
                        ui.vertical(|ui| match tc {
                            Some((y, x)) => {
                                let mut is_tc = true;
                                ui.checkbox(&mut is_tc, i.to_string());
                                if is_tc {
                                    ui.horizontal(|ui| {
                                        ui.label("y");
                                        ui.add(DragValue::new(y).speed(1.0));
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("x");
                                        ui.add(DragValue::new(x).speed(1.0));
                                    });
                                } else {
                                    *tc = None;
                                }
                            }
                            None => {
                                let mut is_tc = false;
                                if ui.checkbox(&mut is_tc, i.to_string()).changed() && is_tc {
                                    *tc = Some((0, 0));
                                }
                            }
                        });
                    });
                }
            })
            .body(|mut body| {
                for (i, daq_row) in daq_data.data().rows().into_iter().enumerate() {
                    body.row(20.0, |mut row| {
                        row.col(|ui| {
                            let mut button =
                                Button::new(i.to_string()).min_size(egui::vec2(CELL_WIDTH, 0.0));
                            if i == self.row_index {
                                button = button.fill(Color32::LIGHT_RED);
                            }
                            if ui.add(button).clicked() {
                                self.row_index = i;
                            }
                        });

                        for v in daq_row {
                            row.col(|ui| {
                                let mut text = RichText::new(format!("{v:.2}"));
                                if i == self.row_index {
                                    text = text.color(Color32::LIGHT_RED);
                                }
                                ui.label(text);
                            });
                        }
                    });
                }
            });
    }

    fn render_synchronization(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.heading("同步");

            let Some((_, Promise::Ready(Ok(video_data)))) = &mut self.video else { return };
            let Some((_, Promise::Ready(Ok(daq_data)))) = &mut self.daq else { return };

            let start_index_old = self.start_index;

            match &mut self.start_index {
                Some(start_index) => {
                    if ui.button("重新同步").clicked() {
                        *start_index = StartIndex {
                            start_frame: self.frame_index,
                            start_row: self.row_index,
                        };
                    }

                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::GREEN, "✔︎");
                        let nframes = video_data.nframes();
                        let nrows = daq_data.data().nrows();
                        let StartIndex {
                            mut start_frame,
                            mut start_row,
                        } = *start_index;
                        let (start_frame_old, start_row_old) = (start_frame, start_row);
                        ui.label("起始帧数");
                        if ui
                            .add(DragValue::new(&mut start_frame).speed(1.0))
                            .changed()
                        {
                            if start_frame > nframes
                                || start_row_old + start_frame < start_frame_old
                            {
                                return;
                            }
                            start_row = start_row_old + start_frame - start_frame_old;
                            if start_row > nrows {
                                return;
                            }
                            *start_index = StartIndex {
                                start_frame,
                                start_row,
                            };
                        }
                        ui.label("起始行数");
                        if ui.add(DragValue::new(&mut start_row).speed(1.0)).changed() {
                            if start_row > nrows || start_frame_old + start_row < start_row_old {
                                return;
                            }
                            let start_frame = start_frame_old + start_row - start_row_old;
                            if start_frame > nframes {
                                return;
                            }
                            *start_index = StartIndex {
                                start_frame,
                                start_row,
                            };
                        }
                    });
                }
                None => {
                    if ui.button("确认同步").clicked() {
                        self.start_index = Some(StartIndex {
                            start_frame: self.frame_index,
                            start_row: self.row_index,
                        });
                    }
                }
            }

            // TODO: debounce.
            if self.start_index != start_index_old {
                let Some(start_index) = self.start_index else { return };
                let Some(area) = self.area else { return };

                let cal_num =
                    eval_cal_num(video_data.nframes(), daq_data.data().nrows(), start_index);
                let video_data = video_data.clone();
                self.green2 = Some(Promise::spawn(move || {
                    video_data.decode_range_area(start_index.start_frame, cal_num, area)
                }));
            }
        });
    }

    fn render_green2(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.heading("绿值矩阵");

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
        });
    }

    fn render_peak_detection(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.heading("峰值检测");

            let filter_method = self.filter_method;
            ComboBox::from_label("选择滤波方法")
                .selected_text(match self.filter_method {
                    FilterMethod::No => "不滤波",
                    FilterMethod::Median { .. } => "中值",
                    FilterMethod::Wavelet { .. } => "小波",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filter_method, FilterMethod::No, "不滤波");
                    ui.selectable_value(
                        &mut self.filter_method,
                        FilterMethod::Median { window_size: 5 },
                        "中值",
                    );
                    ui.selectable_value(
                        &mut self.filter_method,
                        FilterMethod::Wavelet {
                            threshold_ratio: 0.1,
                        },
                        "小波",
                    );
                });

            match self.filter_method {
                FilterMethod::Median { mut window_size } => {
                    ui.horizontal(|ui| {
                        ui.label("窗口");
                        if ui
                            .add(
                                DragValue::new(&mut window_size)
                                    .clamp_range(1..=100)
                                    .speed(1),
                            )
                            .changed()
                        {
                            self.filter_method = FilterMethod::Median { window_size };
                        }
                    });
                }
                FilterMethod::Wavelet {
                    mut threshold_ratio,
                } => {
                    ui.horizontal(|ui| {
                        ui.label("阈值比例");
                        if ui
                            .add(
                                DragValue::new(&mut threshold_ratio)
                                    .clamp_range(0.01..=0.99)
                                    .speed(0.01),
                            )
                            .changed()
                        {
                            self.filter_method = FilterMethod::Wavelet { threshold_ratio };
                        }
                    });
                }
                _ => {}
            }

            if filter_method != self.filter_method {
                let Some(area) = self.area else { return };
                let Some(Promise::Ready(Ok(green2))) = &self.green2 else { return };

                let filter_method = self.filter_method;
                {
                    let green2 = green2.clone();
                    let position = (100u32, 300u32);
                    self.point_green_history = Some((
                        position,
                        Promise::spawn(move || filter_point(green2, filter_method, area, position)),
                    ));
                }

                let green2 = green2.clone();
                self.gmax_frame_indexes = Some(Promise::spawn(move || {
                    filter_detect_peak(green2, filter_method)
                }));
            }

            if let Some((position, promise)) = &self.point_green_history {
                match promise {
                    Promise::Pending(output) => match output.take() {
                        Some(ret) => {
                            self.point_green_history = Some((*position, Promise::Ready(ret)))
                        }
                        None => _ = ui.spinner(),
                    },
                    Promise::Ready(ret) => match ret {
                        Ok(green_history) => {
                            use egui::plot::{Line, Plot};
                            let line = Line::new(
                                green_history
                                    .iter()
                                    .enumerate()
                                    .map(|(i, v)| [i as f64, *v as f64])
                                    .collect::<Vec<_>>(),
                            );
                            Plot::new("point green history")
                                .height(100.0)
                                .show(ui, |plot_ui| plot_ui.line(line));
                        }
                        Err(e) => _ = ui.label(e.to_string()),
                    },
                }
            }

            if let Some(promise) = &self.gmax_frame_indexes {
                match promise {
                    Promise::Pending(output) => match output.take() {
                        Some(gmax_frame_indexes) => {
                            self.gmax_frame_indexes = Some(Promise::Ready(gmax_frame_indexes));
                        }
                        None => _ = ui.spinner(),
                    },
                    Promise::Ready(_gmax_frame_indexes) => {
                        _ = ui.colored_label(Color32::GREEN, "✔︎")
                    }
                }
            }
        });
    }
}

impl eframe::App for Tlc {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::both().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ScrollArea::both()
                        .max_width(360.0)
                        .min_scrolled_height(768.0)
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                self.render_experiment_name(ui);
                                ui.separator();
                                self.render_video_selector(ui);
                                ui.separator();
                                self.render_daq_selector(ui);
                                ui.separator();
                                self.render_synchronization(ui);
                                ui.separator();
                                self.render_green2(ui);
                                ui.separator();
                                self.render_peak_detection(ui);
                            });
                        });

                    ui.vertical(|ui| {
                        self.render_video_frame(ui);
                        self.render_daq_table(ui);
                    });
                });
            });
        });
    }
}

fn eval_cal_num(nframes: usize, nrows: usize, start_index: StartIndex) -> usize {
    let start_frame = start_index.start_frame;
    let start_row = start_index.start_row;
    (nframes - start_frame).min(nrows - start_row)
}
