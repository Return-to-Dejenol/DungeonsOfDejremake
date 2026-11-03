use std::fs;

use ddj_rules::Tuning;
use eframe::egui;

struct App {
    tuning: Result<Tuning, String>,
    data: Vec<String>,
    import_path: String,
    status: String,
}

impl App {
    fn new() -> Self {
        let mut app = Self { tuning: load_tuning(), data: Vec::new(), import_path: String::new(), status: String::new() };
        app.scan();
        app
    }

    fn scan(&mut self) {
        self.data = fs::read_dir(ddj_extract::data_dir())
            .map(|d| d.flatten().filter_map(|e| e.file_name().into_string().ok()).collect())
            .unwrap_or_default();
        self.data.sort();
    }
}

/// Player patches: every `patches/*.toml` except the built-in `modern.toml`, by name.
fn load_tuning() -> Result<Tuning, String> {
    let mut paths: Vec<_> = fs::read_dir("patches")
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    paths.retain(|p| p.extension().is_some_and(|e| e == "toml") && !p.ends_with("modern.toml"));
    paths.sort();
    let texts: Vec<String> = paths.iter().filter_map(|p| fs::read_to_string(p).ok()).collect();
    Tuning::load(&texts.iter().map(String::as_str).collect::<Vec<_>>()).map_err(|e| e.to_string())
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Depths of Dejenol remake");
            ui.separator();
            if self.data.iter().any(|f| f == "MDATA1.MDR") {
                let mids = self.data.iter().filter(|f| f.ends_with(".MID")).count();
                let wavs = self.data.iter().filter(|f| f.ends_with(".WAV")).count();
                ui.label(format!("Game data imported: {} files ({mids} music, {wavs} sounds).", self.data.len()));
            } else {
                ui.label("Point this at your copy of the original game (MORDOR11.ZIP, MORDOR.WIP, or the install folder):");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.import_path);
                    if ui.button("Import").clicked() {
                        self.status = match ddj_extract::import(self.import_path.trim().as_ref(), &ddj_extract::data_dir()) {
                            Ok(f) => format!("Imported {} files.", f.len()),
                            Err(e) => format!("Import failed: {e}"),
                        };
                        self.scan();
                    }
                });
                ui.label(&self.status);
            }
            ui.separator();
            match &self.tuning {
                Ok(t) => {
                    ui.label(format!(
                        "XP to next level: L2 {}, L10 {}, L50 {}",
                        t.xp_to_next(2),
                        t.xp_to_next(10),
                        t.xp_to_next(50)
                    ));
                    let locks: Vec<String> = (1..=15).map(|f| format!("{:.0}%", t.magic_lock_chance(f) * 100.0)).collect();
                    ui.label(format!("Magic-locked chest chance, floors 1-15: {}", locks.join(" ")));
                }
                Err(e) => {
                    ui.colored_label(egui::Color32::RED, format!("Bad patch file: {e}"));
                }
            }
        });
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "Depths of Dejenol remake",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(App::new()))),
    )
}
