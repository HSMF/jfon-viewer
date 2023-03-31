use std::collections::HashMap;

use eframe::egui::{
    self,
    plot::{BoxElem, BoxPlot, BoxSpread, Legend, Plot},
};

fn main() -> Result<(), eframe::Error> {
    // Log to stdout (if you run with `RUST_LOG=debug`).
    // tracing_subscriber::fmt::init();

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(320.0, 240.0)),
        ..Default::default()
    };
    eframe::run_native(
        "JFON viewer",
        options,
        Box::new(|_cc| Box::<Analyzer>::default()),
    )
}

struct Timespan {
    start: u64,
    duration: u64,
}

enum EventKind {
    Seqno { seqno: u32 },
}

struct Event {
    kind: EventKind,
    span: Timespan,
}

struct Analyzer {
    filename: String,
    events: Vec<Event>,
}

impl Analyzer {
    fn read(&mut self) {
        let mut seqs = HashMap::<_, (Option<u64>, Option<u64>)>::new();
        let Ok( c ) = std::fs::read_to_string(&self.filename) else {return;};
        for line in c.lines() {
            if let Some(rest) = line.strip_prefix("seqno:") {
                let mut parts = rest.split(',');
                let seqno = parts.next().unwrap();
                let seqno = seqno.parse::<u32>().unwrap();

                let action = parts.next().unwrap();

                let time = parts.next().unwrap().parse::<u64>().unwrap();

                let entry = seqs.entry(seqno).or_default();
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

        self.events = seqs
            .iter()
            .map(|(&seqno, &(start, end))| {
                let start = start.unwrap();
                let duration = end.map(|e| e - start).unwrap_or(1000);
                Event {
                    kind: EventKind::Seqno { seqno },
                    span: Timespan {
                        start: start - min,
                        duration,
                    },
                }
            })
            .collect();

        self.events.sort_by(|a, b| a.span.start.cmp(&b.span.start))
    }

    fn new() -> Self {
        let filename = std::env::args()
            .nth(2)
            .unwrap_or("../out/7874.jfon".to_owned());
        let mut out = Self {
            filename,
            events: vec![],
        };

        out.read();

        out
    }
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for Analyzer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("JFON viewer");
            ui.horizontal(|ui| {
                let name_label = ui.label("filename: ");
                ui.text_edit_singleline(&mut self.filename)
                    .labelled_by(name_label.id);
            });

            if ui.button("reload").clicked() {
                self.read();
            }

            Plot::new("bars")
                .legend(Legend::default())
                .data_aspect(10.0)
                .show(ui, |pui| {
                    for ev in &self.events {
                        match ev.kind {
                            EventKind::Seqno { seqno } => {
                                let e = BoxElem::new(
                                    seqno as f64,
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
                                    BoxPlot::new(vec![e]).horizontal().name(format!("{seqno}")),
                                )
                            }
                        }
                    }
                })
        });
    }
}
