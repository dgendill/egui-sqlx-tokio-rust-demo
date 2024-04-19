/// An example of a simple application that uses eframe,
/// egui, sqlx with a sqlite , and tokio
use std::sync::mpsc::{Receiver, Sender};
use tokio::runtime::Runtime;

fn main() -> eframe::Result<()> {
    // Create the tokio runtime manually
    // https://github.com/emilk/egui/discussions/521#discussioncomment-3462382

    let rt = Runtime::new().expect("Unable to create Runtime");
    let _enter = rt.enter();

    // Execute the runtime in its own thread.
    // The future doesn't have to do anything. In this example, it just sleeps forever.
    std::thread::spawn(move || {
        rt.block_on(async {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        })
    });

    // Setup the eframe window size
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_min_inner_size([300.0, 220.0]),
        ..Default::default()
    };

    // Run egui natively
    eframe::run_native(
        "eframe template",
        native_options,
        Box::new(|_| Box::new(App::new())),
    )
}

// The messages that can be passed between the
// main egui thread and the tokio thread
pub enum AppMessage {
    ApplicationLoad(Vec<String>),
    ItemAdded(String),
}

// The state of the application
// and a sender and receiver
struct App {
    pub tx: Sender<AppMessage>,
    pub rx: Receiver<AppMessage>,
    pub items: Vec<String>,
    pub new_item_name: String,
}

impl Default for App {
    fn default() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();

        App {
            tx,
            rx,
            items: vec![],
            new_item_name: "".to_string(),
        }
    }
}
impl App {
    fn new() -> Self {
        let app: Self = Default::default();
        let tx = app.tx.clone();

        // Initialize the database upon app creation
        init_database(tx);

        app
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle message from the tokio thread
        match self.rx.try_recv() {
            Ok(AppMessage::ApplicationLoad(items)) => {
                println!("Application loaded items: {:?}", items);
                self.items = items;
            }
            Ok(AppMessage::ItemAdded(item)) => {
                println!("Item added: {:?}", item);
                self.items.insert(0, item);
                self.new_item_name = String::new();
            }
            _ => {}
        }

        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let button = egui::Button::new("Create Item");
                let input = ui.text_edit_singleline(&mut self.new_item_name);

                // Handle click
                if ui.add(button).clicked() && !self.new_item_name.is_empty() {
                    add_item(self.new_item_name.to_string(), self.tx.clone());
                }

                // Handle enter
                if input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    add_item(self.new_item_name.to_string(), self.tx.clone());
                }
            });

            for a in &self.items {
                ui.label(a);
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Hello World");
        });
    }
}

use async_once_cell::OnceCell;
use sqlx::{migrate::MigrateDatabase, sqlite::SqlitePoolOptions, Executor, Pool, Row, Sqlite};
static POOL: OnceCell<Pool<Sqlite>> = OnceCell::new();
static URL: &str = "sqlite://items.db";

async fn get_pool<'a>(url: &str) -> &'a Pool<Sqlite> {
    POOL.get_or_init(async {
        SqlitePoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .expect("Could not create DB Pool")
    })
    .await
}

// Spawning a tokio thread and initialize the database
fn init_database(tx: Sender<AppMessage>) {
    tokio::spawn(async move {
        if !Sqlite::database_exists(URL).await.unwrap_or(false) {
            Sqlite::create_database(URL)
                .await
                .expect("Could not create DB");
        }

        let pool = get_pool(URL).await;

        pool.execute(
            r#"
      CREATE TABLE IF NOT EXISTS item (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL
      );        
    "#,
        )
        .await
        .expect("Could not create DB Schema");

        let items: Vec<String> = sqlx::query("SELECT name FROM item")
            .fetch_all(pool)
            .await
            .expect("Could not query item table")
            .iter()
            .map(|row| String::from(row.get::<&str, usize>(0)))
            .collect();

        let _ = tx.send(AppMessage::ApplicationLoad(items));
    });
}

// Spawning a tokio thread and add an item to the database
fn add_item(name: String, tx: Sender<AppMessage>) {
    tokio::spawn(async move {
        let pool = get_pool(URL).await;

        let item = sqlx::query(
            r#"
INSERT INTO item (name) VALUES(?);        
"#,
        )
        .bind(&name)
        .execute(pool)
        .await;

        if item.is_ok() {
            let _ = tx.send(AppMessage::ItemAdded(name.to_string()));
        }
    });
}
