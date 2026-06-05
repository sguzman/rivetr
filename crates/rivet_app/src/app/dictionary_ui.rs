use egui::{Color32, RichText};
use std::sync::mpsc;
use std::thread;
use postgres::{Client, NoTls};

use crate::types::RuntimeDictionaryConfig;

#[derive(Clone)]
pub struct DictionaryEntry {
    pub word: String,
    pub definition: String,
}

pub struct DictionaryUi {
    pub search_query: String,
    pub results: Vec<DictionaryEntry>,
    pub is_searching: bool,
    pub error_msg: Option<String>,
    rx: Option<mpsc::Receiver<Result<Vec<DictionaryEntry>, String>>>,
    config: Option<RuntimeDictionaryConfig>,
}

impl Default for DictionaryUi {
    fn default() -> Self {
        Self {
            search_query: String::new(),
            results: Vec::new(),
            is_searching: false,
            error_msg: None,
            rx: None,
            config: None,
        }
    }
}

impl DictionaryUi {
    pub fn new(config: Option<RuntimeDictionaryConfig>) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    pub fn render(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Check for search results
        if let Some(rx) = &self.rx {
            if let Ok(res) = rx.try_recv() {
                self.is_searching = false;
                self.rx = None;
                match res {
                    Ok(entries) => {
                        self.results = entries;
                        self.error_msg = None;
                    }
                    Err(e) => {
                        self.error_msg = Some(e);
                        self.results.clear();
                    }
                }
            }
        }

        ui.heading("Dictionary");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text("Search for a word...")
                    .desired_width(300.0),
            );

            if ui
                .add_enabled(!self.is_searching && !self.search_query.is_empty(), egui::Button::new("Search"))
                .clicked()
                || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                self.start_search(ctx.clone());
            }
        });

        ui.add_space(16.0);

        if self.is_searching {
            ui.spinner();
            ui.label("Searching...");
        } else if let Some(err) = &self.error_msg {
            ui.label(RichText::new(format!("Error: {}", err)).color(Color32::RED));
        } else {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if self.results.is_empty() && !self.search_query.is_empty() && self.error_msg.is_none() && self.rx.is_none() {
                    ui.label("No results found.");
                } else {
                    for entry in &self.results {
                        ui.group(|ui| {
                            ui.set_width(ui.available_width());
                            ui.heading(&entry.word);
                            ui.add_space(4.0);
                            ui.label(&entry.definition);
                        });
                        ui.add_space(8.0);
                    }
                }
            });
        }
    }

    fn start_search(&mut self, ctx: egui::Context) {
        if self.search_query.is_empty() {
            return;
        }

        self.is_searching = true;
        self.error_msg = None;
        self.results.clear();

        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);

        let query = self.search_query.clone();
        
        // We clone config to move it to the thread
        let config_clone = self.config.clone();

        thread::spawn(move || {
            let result = run_search(&query, config_clone);
            let _ = tx.send(result);
            ctx.request_repaint(); // Wake up the UI
        });
    }
}

fn run_search(word: &str, config_opt: Option<RuntimeDictionaryConfig>) -> Result<Vec<DictionaryEntry>, String> {
    let config = config_opt.ok_or_else(|| "Dictionary configuration is missing".to_string())?;
    
    // We use the configured IP if any. The user explicitly asked to use 192.168.0.113 in the code
    // because they don't want us to modify the reference project config.
    let pg_conf = config.postgres.ok_or_else(|| "Postgres configuration is missing".to_string())?;
    
    // Override the host to 192.168.0.113 if it was 127.0.0.1 based on user request.
    let host = if let Some(h) = &pg_conf.host {
        if h == "127.0.0.1" {
            "192.168.0.113".to_string()
        } else {
            h.clone()
        }
    } else {
        "192.168.0.113".to_string()
    };
    
    let port = pg_conf.port.unwrap_or(5432);
    let user = pg_conf.user.as_deref().unwrap_or("admin");
    let password = pg_conf.password.as_deref().unwrap_or("admin");
    let database = pg_conf.database.as_deref().unwrap_or("data");
    let schema = pg_conf.schema.as_deref().unwrap_or("dictionary");

    let conn_str = format!(
        "host={} port={} user={} password={} dbname={}",
        host, port, user, password, database
    );

    let mut client = Client::connect(&conn_str, NoTls)
        .map_err(|e| format!("Failed to connect to database at {}: {}", host, e))?;

    let max_results = config.max_results.unwrap_or(100) as i64;

    // Use ILIKE for prefix search by default since that's the configured search_mode
    let search_term = format!("{}%", word);

    let sql = format!(
        "SELECT word, definition FROM {}.entries WHERE word ILIKE $1 LIMIT $2",
        schema
    );

    let rows = client.query(&sql, &[&search_term, &max_results])
        .map_err(|e| format!("Query error: {}", e))?;

    let mut entries = Vec::new();
    for row in rows {
        let w: String = row.try_get(0).unwrap_or_else(|_| "Unknown".to_string());
        let def: String = row.try_get(1).unwrap_or_else(|_| "No definition found.".to_string());
        entries.push(DictionaryEntry {
            word: w,
            definition: def,
        });
    }

    Ok(entries)
}
