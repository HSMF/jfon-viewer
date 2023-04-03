use std::future::Future;
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    sync::Arc,
};

use parking_lot::Mutex;

use eframe::egui::{
    self,
    plot::{BoxElem, BoxPlot, BoxSpread, Legend, Plot},
    ComboBox,
};
use eframe::egui::{Layout, RichText};
use eframe::epaint::Color32;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), eframe::Error> {
    // Log to stdout (if you run with `RUST_LOG=debug`).
    tracing_subscriber::fmt::init();

    let options = eframe::NativeOptions {
        // initial_window_size: Some(egui::vec2(320.0, 240.0)),
        ..Default::default()
    };
    eframe::run_native(
        "JFON viewer",
        options,
        Box::new(|_cc| {
            let mut analyzer = Analyzer::new();
            if let Some(filename) = std::env::args().nth(1) {
                analyzer.filename = filename;
                analyzer.read();
            }
            Box::new(analyzer)
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();

    // Redirect tracing to console.log and friends:
    tracing_wasm::set_as_global_default();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        eframe::start_web(
            "main-canvas",
            web_options,
            Box::new(|_cc| Box::<Analyzer>::default()),
        )
        .await
        .expect("failed to start eframe");
    });
}

#[derive(Debug)]
struct Timespan {
    start: u64,
    duration: u64,
}

#[derive(Debug)]
struct Event {
    kind: String,
    id: u32,
    span: Timespan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ViewBy {
    Any,
    Label(String),
}

impl Display for ViewBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewBy::Any => write!(f, "Any"),
            ViewBy::Label(x) => write!(f, "Label: {x}"),
        }
    }
}

impl ViewBy {
    fn matching(&self, other: &Event) -> bool {
        match self {
            ViewBy::Any => true,
            ViewBy::Label(l) => &other.kind == l,
        }
    }
}

#[derive(Debug, Default)]
struct Events {
    events: Vec<Event>,
    labels: Vec<String>,
}

impl Events {
    fn read(data: &str) -> Result<Self, Error> {
        let mut seqs = HashMap::<_, (Option<u64>, Option<u64>)>::new();
        let mut labels = HashSet::new();
        for (line_number, line) in data.lines().enumerate() {
            use FmtErrorKind::*;
            if let Some((label, rest)) = line.split_once(':') {
                labels.insert(label);
                let mut parts = rest.split(',');
                let seqno = parts
                    .next()
                    .ok_or(Error::FormatError {
                        line_number,
                        kind: SyntaxError,
                    })?
                    .parse::<u32>()
                    .map_err(|_| Error::FormatError {
                        line_number,
                        kind: SyntaxError,
                    })?;

                let action = parts.next().ok_or(Error::FormatError {
                    line_number,
                    kind: SyntaxError,
                })?;

                let time = parts
                    .next()
                    .ok_or(Error::FormatError {
                        line_number,
                        kind: SyntaxError,
                    })?
                    .parse::<u64>()
                    .map_err(|_| Error::FormatError {
                        line_number,
                        kind: SyntaxError,
                    })?;

                let entry = seqs.entry((label, seqno)).or_default();
                match action {
                    "start" => entry.0 = Some(time),
                    "end" => entry.1 = Some(time),
                    x => {
                        return Err(Error::FormatError {
                            line_number,
                            kind: InvalidAction(x.to_owned()),
                        })
                    }
                }
            }
        }

        let min = *seqs
            .iter()
            .filter_map(|(&_, (start, _))| start.as_ref())
            .min()
            .unwrap_or(&0);

        let mut events: Vec<_> = seqs
            .iter()
            .filter_map(|(key, &(start, end))| start.map(|start| (key, (start, end))))
            .map(|(&(label, id), (start, end))| {
                let duration = end.map(|e| e - start).unwrap_or(1000);
                Event {
                    kind: label.to_string(),
                    id,
                    span: Timespan {
                        start: start - min,
                        duration,
                    },
                }
            })
            .collect();

        events.sort_by(|a, b| a.span.start.cmp(&b.span.start));
        Ok(Events {
            events,
            labels: labels.iter().copied().map(ToOwned::to_owned).collect(),
        })
    }
}

#[derive(Debug)]
struct Analyzer {
    #[cfg(not(target_arch = "wasm32"))]
    filename: String,
    events: Arc<Mutex<Events>>,
    error: Arc<Mutex<Option<Error>>>,
    view_by: ViewBy,
}

#[derive(Debug)]
pub enum FmtErrorKind {
    InvalidAction(String),
    SyntaxError,
}

#[derive(Debug)]
pub enum Error {
    IoError(std::io::Error),
    FormatError {
        line_number: usize,
        kind: FmtErrorKind,
    },
}
impl Error {
    fn show(&self, ui: &mut egui::Ui) {
        const COLOR: Color32 = Color32::from_rgb(200, 0, 0);
        match self {
            Error::IoError(err) => {
                ui.colored_label(COLOR, format!("{err}"));
            }
            Error::FormatError { line_number, kind } => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Error on Line").color(COLOR));
                    ui.label(
                        RichText::new(format!("{line_number}"))
                            .color(COLOR)
                            .monospace(),
                    );
                    ui.label(RichText::new(":").color(COLOR));
                    match kind {
                        FmtErrorKind::InvalidAction(action) => {
                            ui.label(
                                RichText::new(format!("invalid action: `{action}`")).color(COLOR),
                            );
                        }
                        FmtErrorKind::SyntaxError => {
                            ui.label(RichText::new("Syntax Error!").color(COLOR));
                        }
                    }
                });
            }
        }
    }
}

impl Analyzer {
    #[cfg(not(target_arch = "wasm32"))]
    fn read(&mut self) {
        let c = match std::fs::read_to_string(&self.filename) {
            Ok(c) => c,
            Err(e) => {
                *self.error.lock() = Some(Error::IoError(e));
                return;
            }
        };
        match Events::read(&c) {
            Ok(events) => {
                self.events = Arc::new(Mutex::new(events));
            }
            Err(e) => *self.error.lock() = Some(e),
        }
    }

    fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            filename: String::new(),
            events: Arc::new(Mutex::new(Events::default())),
            view_by: ViewBy::Any,
            error: Arc::new(Mutex::new(None)),
        }
    }
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn execute<F: Future<Output = ()> + Send + 'static>(f: F) {
    // this is stupid... use any executor of your choice instead

    std::thread::spawn(move || futures::executor::block_on(f));
}
#[cfg(target_arch = "wasm32")]
fn execute<F: Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
}

impl eframe::App for Analyzer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("JFON viewer");
            #[cfg(not(target_arch = "wasm32"))]
            ui.horizontal(|ui| {
                use eframe::egui::Key;
                let name_label = ui.label("filename: ");
                let input = ui
                    .text_edit_singleline(&mut self.filename)
                    .labelled_by(name_label.id);
                if input.lost_focus() && input.ctx.input(|r| r.key_down(Key::Enter)) {
                    self.read()
                }
            });

            ui.horizontal(|ui| {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if ui.button("reload").clicked() {
                        self.read();
                    }
                }
                if ui.button("Open file…").clicked() {
                    let task = rfd::AsyncFileDialog::new()
                        .add_filter("jfon", &["jfon"])
                        .pick_file();
                    let events = Arc::clone(&self.events);
                    let error = Arc::clone(&self.error);
                    execute(async move {
                        let file = task.await;
                        if let Some(file) = file {
                            #[cfg(not(target_arch = "wasm32"))]
                            log::info!("loading {:?}", file.path());

                            let data = file.read().await;
                            match Events::read(String::from_utf8(data).unwrap().as_str()) {
                                Ok(e) => {
                                    *events.lock() = e;
                                }
                                Err(e) => *error.lock() = Some(e),
                            }
                        }
                    });
                }
            });

            ui.horizontal(|ui| {
                ui.label("View By: ");
                ComboBox::from_label("")
                    .selected_text(self.view_by.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.view_by, ViewBy::Any, "Any");

                        for label in &self.events.lock().labels {
                            let val = ViewBy::Label(label.clone());
                            let s = val.to_string();
                            ui.selectable_value(&mut self.view_by, val, s);
                        }
                    });

                ui.with_layout(Layout::right_to_left(eframe::emath::Align::Max), |ui| {
                    ui.hyperlink_to(
                        "github.com/HSMF/jfon-viewer",
                        "https://github.com/HSMF/jfon-viewer",
                    )
                });
            });

            if let Some(ref error) = *self.error.lock() {
                error.show(ui)
            } else if self.events.lock().events.is_empty() {
                ui.label("Load some data to get started");
            } else {
                Plot::new("bars")
                    .legend(Legend::default())
                    .data_aspect(10.0)
                    .show(ui, |pui| {
                        for ev in &self.events.lock().events {
                            if self.view_by.matching(ev) {
                                let id = ev.id;
                                let e = BoxElem::new(
                                    id as f64,
                                    BoxSpread::new(
                                        ev.span.start as f64,
                                        ev.span.start as f64,
                                        ev.span.start as f64,
                                        (ev.span.start + ev.span.duration) as f64,
                                        (ev.span.start + ev.span.duration) as f64,
                                    ),
                                )
                                .box_width(1.0);
                                pui.box_plot(
                                    BoxPlot::new(vec![e]).horizontal().name(format!("{id}")),
                                )
                            }
                        }
                    });
            }
        });
    }
}
