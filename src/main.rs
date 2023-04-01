use std::future::Future;
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    sync::{Arc, Mutex},
};

use eframe::egui::Layout;
use eframe::egui::{
    self,
    plot::{BoxElem, BoxPlot, BoxSpread, Legend, Plot},
    ComboBox,
};

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), eframe::Error> {
    // Log to stdout (if you run with `RUST_LOG=debug`).
    tracing_subscriber::fmt::init();

    let options = eframe::NativeOptions {
        // initial_window_size: Some(egui::vec2(320.0, 240.0)),
        ..Default::default()
    };
    let filename = std::env::args().nth(1).unwrap_or("".to_owned());
    eframe::run_native(
        "JFON viewer",
        options,
        Box::new(|_cc| {
            let mut analyzer = Analyzer::new();
            analyzer.filename = filename;
            analyzer.read();
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
    fn read(data: &str) -> Option<Self> {
        let mut seqs = HashMap::<_, (Option<u64>, Option<u64>)>::new();
        let mut labels = HashSet::new();
        for line in data.lines() {
            if let Some((label, rest)) = line.split_once(':') {
                labels.insert(label);
                let mut parts = rest.split(',');
                let seqno = parts.next()?.parse::<u32>().ok()?;

                let action = parts.next()?;

                let time = parts.next()?.parse::<u64>().ok()?;

                let entry = seqs.entry((label, seqno)).or_default();
                match action {
                    "start" => entry.0 = Some(time),
                    "end" => entry.1 = Some(time),
                    x => panic!("unknown action {x:?}"),
                }
            }
        }

        let min = seqs
            .iter()
            .map(|(&_, &(start, _))| start.unwrap())
            .min()
            .unwrap_or(0);

        let mut events: Vec<_> = seqs
            .iter()
            .map(|(&(label, id), &(start, end))| {
                let start = start.unwrap();
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
        Some(Events {
            events,
            labels: labels.iter().copied().map(ToOwned::to_owned).collect(),
        })
    }
}

#[derive(Debug)]
struct Analyzer {
    filename: String,
    events: Arc<Mutex<Events>>,
    view_by: ViewBy,
}

impl Analyzer {
    fn read(&mut self) {
        let Ok( c ) = std::fs::read_to_string(&self.filename) else {return;};
        if let Some(events) = Events::read(&c) {
            self.events = Arc::new(Mutex::new(events));
        }
    }

    fn new() -> Self {
        Self {
            filename: String::new(),
            events: Arc::new(Mutex::new(Events::default())),
            view_by: ViewBy::Any,
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
                if ui.button("reload").clicked() {
                    self.read();
                }
                if ui.button("Open file…").clicked() {
                    let task = rfd::AsyncFileDialog::new()
                        .add_filter("jfon", &["jfon"])
                        .pick_file();
                    let events = Arc::clone(&self.events);
                    execute(async move {
                        let file = task.await;
                        if let Some(file) = file {
                            #[cfg(not(target_arch = "wasm32"))]
                            log::info!("loading {:?}", file.path());

                            let data = file.read().await;
                            if let Some(e) = Events::read(String::from_utf8(data).unwrap().as_str())
                            {
                                *events.lock().unwrap() = e;
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

                        for label in &self.events.lock().unwrap().labels {
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

            if self.events.lock().unwrap().events.is_empty() {
                ui.label("Load some data to get started");
            } else {
                Plot::new("bars")
                    .legend(Legend::default())
                    .data_aspect(10.0)
                    .show(ui, |pui| {
                        for ev in &self.events.lock().unwrap().events {
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
