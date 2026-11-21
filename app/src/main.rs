use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    thread,
    time::SystemTime,
};
use walkdir::WalkDir;

#[derive(Clone, Debug)]
struct FileRecord {
    path: PathBuf,
    name: String,
    is_dir: bool,
    size: u64,
    modified: Option<SystemTime>,
}

enum ScanMessage { Entry(FileRecord), Finished { visited: u64, errors: u64 } }

struct UltraSearchApp {
    roots: Vec<PathBuf>,
    entries: Vec<FileRecord>,
    visible: Vec<usize>,
    query: String,
    selected: Option<usize>,
    preview: String,
    status: String,
    scanning: bool,
    receiver: Option<Receiver<ScanMessage>>,
    cancel: Option<Arc<AtomicBool>>,
    scanned: u64,
    scan_errors: u64,
    case_sensitive: bool,
    include_folders: bool,
}

impl Default for UltraSearchApp {
    fn default() -> Self {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        Self {
            roots: vec![home], entries: Vec::new(), visible: Vec::new(), query: String::new(),
            selected: None, preview: String::new(), status: "Choose a folder, then start indexing.".into(),
            scanning: false, receiver: None, cancel: None, scanned: 0, scan_errors: 0,
            case_sensitive: false, include_folders: false,
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("UltraSearch — Cross-platform file search").with_inner_size([1120.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native("UltraSearch", options, Box::new(|_cc| Box::new(UltraSearchApp::default())))
}

impl UltraSearchApp {
    fn start_scan(&mut self) {
        if self.scanning || self.roots.is_empty() { return; }
        let roots = self.roots.clone();
        let (tx, rx) = unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        thread::Builder::new().name("ultrasearch-indexer".into()).spawn(move || scan_roots(roots, tx, worker_cancel)).ok();
        self.entries.clear(); self.visible.clear(); self.selected = None; self.preview.clear();
        self.scanned = 0; self.scan_errors = 0; self.receiver = Some(rx); self.cancel = Some(cancel);
        self.scanning = true; self.status = "Indexing…".into();
    }

    fn poll_scan(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.receiver else { return; };
        let mut finished = None;
        for _ in 0..4000 {
            match rx.try_recv() {
                Ok(ScanMessage::Entry(entry)) => { self.scanned += 1; self.entries.push(entry); }
                Ok(ScanMessage::Finished { visited, errors }) => { finished = Some((visited, errors)); break; }
                Err(_) => break,
            }
        }
        if let Some((visited, errors)) = finished {
            self.scanning = false; self.receiver = None; self.cancel = None;
            self.scanned = visited; self.scan_errors = errors;
            self.status = format!("Indexed {visited} items · {errors} inaccessible items skipped");
            self.apply_search();
        } else if self.scanning {
            self.status = format!("Indexing… {} items discovered", self.scanned);
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    fn apply_search(&mut self) {
        let terms = parse_query(&self.query);
        let case_sensitive = self.case_sensitive;
        let include_folders = self.include_folders;
        self.visible = self.entries.iter().enumerate().filter_map(|(i, item)| {
            if !include_folders && item.is_dir { return None; }
            if terms.iter().all(|term| term_matches(term, item, case_sensitive)) { Some(i) } else { None }
        }).collect();
        self.selected = self.visible.first().copied();
        self.load_preview();
    }

    fn load_preview(&mut self) {
        self.preview.clear();
        let Some(index) = self.selected else { return; };
        let Some(item) = self.entries.get(index) else { return; };
        if item.is_dir { self.preview = "Folder\n\nSelect a file to preview its contents.".into(); return; }
        let Some(ext) = item.path.extension().and_then(|s| s.to_str()) else { self.preview = "No preview available.".into(); return; };
        let text_ext = ["txt", "md", "rs", "toml", "json", "xml", "csv", "log", "yaml", "yml", "ini", "html", "css", "js", "ts", "py", "sh", "bat", "ps1"];
        if !text_ext.iter().any(|e| ext.eq_ignore_ascii_case(e)) { self.preview = format!("Preview is not available for .{ext} files.\n\nPath: {}\nSize: {}", item.path.display(), human_size(item.size)); return; }
        match fs::read(&item.path) {
            Ok(bytes) => {
                let slice = &bytes[..bytes.len().min(48 * 1024)];
                self.preview = String::from_utf8_lossy(slice).into_owned();
                if bytes.len() > slice.len() { self.preview.push_str("\n\n… preview truncated at 48 KiB"); }
            }
            Err(e) => self.preview = format!("Could not read file: {e}"),
        }
    }
}

fn scan_roots(roots: Vec<PathBuf>, tx: Sender<ScanMessage>, cancel: Arc<AtomicBool>) {
    let mut visited = 0u64; let mut errors = 0u64;
    for root in roots {
        if cancel.load(Ordering::Relaxed) { break; }
        for result in WalkDir::new(root).follow_links(false).into_iter() {
            if cancel.load(Ordering::Relaxed) { break; }
            match result {
                Ok(entry) => {
                    let meta = match entry.metadata() { Ok(m) => m, Err(_) => { errors += 1; continue; } };
                    let path = entry.path().to_path_buf();
                    let is_dir = meta.is_dir();
                    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                    let record = FileRecord { path, name, is_dir, size: if is_dir { 0 } else { meta.len() }, modified: meta.modified().ok() };
                    visited += 1;
                    if tx.send(ScanMessage::Entry(record)).is_err() { return; }
                }
                Err(_) => errors += 1,
            }
        }
    }
    let _ = tx.send(ScanMessage::Finished { visited, errors });
}

#[derive(Debug)]
enum QueryTerm { Plain(String), Name(String), Ext(String), Path(String), Kind(String), SizeGt(u64), SizeLt(u64) }

fn parse_query(query: &str) -> Vec<QueryTerm> {
    query.split_whitespace().filter_map(|raw| {
        let (key, value) = raw.split_once(':').unwrap_or(("", raw));
        if value.is_empty() { return None; }
        Some(match key.to_ascii_lowercase().as_str() {
            "name" => QueryTerm::Name(value.to_string()), "ext" => QueryTerm::Ext(value.trim_start_matches('.').to_string()),
            "path" => QueryTerm::Path(value.to_string()), "type" => QueryTerm::Kind(value.to_ascii_lowercase()),
            _ if raw.starts_with("size>") => parse_size(&raw[5..]).map(QueryTerm::SizeGt)?,
            _ if raw.starts_with("size<") => parse_size(&raw[5..]).map(QueryTerm::SizeLt)?,
            _ => QueryTerm::Plain(raw.to_string()),
        })
    }).collect()
}

fn parse_size(value: &str) -> Option<u64> {
    let upper = value.to_ascii_uppercase();
    let (number, multiplier) = if let Some(v) = upper.strip_suffix("GB") { (v, 1024u64.pow(3)) }
        else if let Some(v) = upper.strip_suffix("MB") { (v, 1024u64.pow(2)) }
        else if let Some(v) = upper.strip_suffix("KB") { (v, 1024) }
        else if let Some(v) = upper.strip_suffix('B') { (v, 1) } else { (upper.as_str(), 1) };
    number.parse::<u64>().ok()?.checked_mul(multiplier)
}

fn term_matches(term: &QueryTerm, item: &FileRecord, case_sensitive: bool) -> bool {
    let path = item.path.to_string_lossy();
    let name = item.name.as_str();
    let contains = |haystack: &str, needle: &str| if case_sensitive { haystack.contains(needle) } else { haystack.to_lowercase().contains(&needle.to_lowercase()) };
    match term {
        QueryTerm::Plain(s) | QueryTerm::Name(s) => contains(name, s),
        QueryTerm::Path(s) => contains(&path, s),
        QueryTerm::Ext(s) => item.path.extension().and_then(|e| e.to_str()).map(|e| if case_sensitive { e == s } else { e.eq_ignore_ascii_case(s) }).unwrap_or(false),
        QueryTerm::Kind(k) => match k.as_str() { "file" => !item.is_dir, "folder" | "dir" | "directory" => item.is_dir, _ => true },
        QueryTerm::SizeGt(n) => item.size > *n, QueryTerm::SizeLt(n) => item.size < *n,
    }
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64; let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 { value /= 1024.0; unit += 1; }
    format!("{value:.1} {}", UNITS[unit])
}

impl eframe::App for UltraSearchApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_scan(ctx);
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("UltraSearch"); ui.separator();
                if ui.button("＋ Add folder").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() { if !self.roots.contains(&path) { self.roots.push(path); } }
                }
                if ui.button("▶ Index folders").clicked() { self.start_scan(); }
                if ui.button("Stop").clicked() { if let Some(cancel) = &self.cancel { cancel.store(true, Ordering::Relaxed); } }
                if ui.button("Clear index").clicked() && !self.scanning { self.entries.clear(); self.visible.clear(); self.selected = None; self.preview.clear(); }
                ui.separator(); ui.label(&self.status);
            });
        });
        egui::SidePanel::left("search_panel").resizable(true).default_width(290.0).show(ctx, |ui| {
            ui.heading("Search");
            let response = ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Search files… (name:, ext:, path:, type:, size>)").desired_width(f32::INFINITY));
            if response.changed() { self.apply_search(); }
            if ui.button("Search").clicked() { self.apply_search(); }
            let mut filters_changed = ui.checkbox(&mut self.case_sensitive, "Case sensitive").changed();
            filters_changed |= ui.checkbox(&mut self.include_folders, "Include folders").changed();
            if filters_changed { self.apply_search(); }
            ui.separator(); ui.label("Indexed locations");
            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                for root in &self.roots { ui.label(root.display().to_string()); }
            });
            ui.separator(); ui.label("Query syntax");
            ui.small("name:report  ext:pdf  path:work\ntype:file  type:folder\nsize>10MB  size<500KB\nTerms are combined with AND.");
            ui.separator(); ui.label(format!("{} indexed · {} matches", self.entries.len(), self.visible.len()));
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.heading("Search results");
                egui::ScrollArea::vertical().id_source("results").max_height(ui.available_height() * 0.53).show(ui, |ui| {
                    egui::Grid::new("results_grid").striped(true).num_columns(3).show(ui, |ui| {
                        ui.strong("Name"); ui.strong("Size"); ui.strong("Location"); ui.end_row();
                        for index in self.visible.iter().take(5000).copied() {
                            let (label, size_text, parent_text) = {
                                let item = &self.entries[index];
                                let label = if item.is_dir { format!("📁 {}", item.name) } else { format!("📄 {}", item.name) };
                                let size = if item.is_dir { "—".to_string() } else { human_size(item.size) };
                                let parent = item.path.parent().unwrap_or(Path::new("")).display().to_string();
                                (label, size, parent)
                            };
                            if ui.selectable_label(self.selected == Some(index), label).clicked() { self.selected = Some(index); self.load_preview(); }
                            ui.label(size_text);
                            ui.label(parent_text); ui.end_row();
                        }
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.heading("Preview");
                    if let Some(index) = self.selected {
                        if let Some(item) = self.entries.get(index) {
                            if ui.button("Open file").clicked() { let _ = open::that(&item.path); }
                            ui.label(item.path.display().to_string());
                        }
                    }
                });
                egui::ScrollArea::vertical().show(ui, |ui| { ui.add(egui::TextEdit::multiline(&mut self.preview).desired_width(f32::INFINITY).desired_rows(8).font(egui::TextStyle::Monospace).interactive(false)); });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FileRecord {
        FileRecord { path: PathBuf::from("/home/test/Annual Report.PDF"), name: "Annual Report.PDF".into(), is_dir: false, size: 2 * 1024 * 1024, modified: None }
    }

    #[test]
    fn parses_size_units() {
        assert_eq!(parse_size("10MB"), Some(10 * 1024 * 1024));
        assert_eq!(parse_size("512KB"), Some(512 * 1024));
        assert_eq!(parse_size("bad"), None);
    }

    #[test]
    fn matches_name_extension_and_size_filters() {
        let item = sample();
        assert!(term_matches(&QueryTerm::Name("Annual".into()), &item, false));
        assert!(term_matches(&QueryTerm::Ext("pdf".into()), &item, false));
        assert!(term_matches(&QueryTerm::SizeGt(1024), &item, false));
        assert!(!term_matches(&QueryTerm::SizeLt(1024), &item, false));
    }

    #[test]
    fn parses_query_operators() {
        let terms = parse_query("name:report ext:pdf size>1MB type:file");
        assert_eq!(terms.len(), 4);
        let item = sample();
        assert!(terms.iter().all(|term| term_matches(term, &item, false)));
    }
}
