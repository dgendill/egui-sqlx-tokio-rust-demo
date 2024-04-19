# egui, sqlx, tokio example

This repo contains my own example of using egui, sqlx, and tokio in a native desktop app. It's intended to further demonstrate the code pattern proposed here in this egui ticket: https://github.com/emilk/egui/discussions/521#discussioncomment-3462382.

To run the demo make sure you have rust and cargo installed. Then run `cargo run` in the project root.

All of the code is in a single file so it's easier to reason about. In a larger application you would want to break the parts into modules. I will explain that later. For now these are the general parts in main.rs...

# Application Singleton

The App struct holds the state of your application. It implements the `eframe::App` trait. In the update function you create panels like `egui::SidePanel` and `egui::CentralPanel`. Inside the panels, you add the ui elements and read and mutate the state.

# Eframe + App Creation

An eframe (the native application window) and the application singleton are executed together to create a window and egui application.

# Database

This project using sqlx with the tokio runtime. The database is accessed via a async static pool `OnceCell<Pool<Sqlite>>`. That type comes from the `async_once_cell` crate, but I think there are plans to have something similar moved into rust's standard library. After the pool is initialized, the same pool will be returned on subsequent calls.

```rust
static POOL: OnceCell<Pool<Sqlite>> = OnceCell::new();

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
```

The application will have functions for basic CRUD.  These functions should look similar to what is proposed in the sqlx documentation or in [this tutorial](https://medium.com/@edandresvan/a-brief-introduction-about-rust-sqlx-5d3cea2e8544).

# Bridge Functionality

Since egui is an immediate mode library, it means that the render function is called many times per second. Long running async tasks should not be run on the main thread. To avoid that, a tokio runtime is created and moved to its own thread. When you need to run an async task from egui, you can spawn a tokio task and trigger
a message when the task is done. For example, to create an item in the database...

```rust
// Spawning a tokio thread and add an item to the database
fn add_item(name: String, tx: Sender<AppMessage>) {
    tokio::spawn(async move {
        let pool = get_pool(URL).await;

        let item = sqlx::query("INSERT INTO item (name) VALUES(?);")
        .bind(&name)
        .execute(pool)
        .await;

        if item.is_ok() {
            let _ = tx.send(AppMessage::ItemAdded(name.to_string()));
        }
    });
}
```

The `Sender` here is from `std::sync::mpsc` (Multi-producer, single-consumer). It has a partner `Receiver` associated it with it. When the sender sends a message the associated receiver gets that message. The message is received in the `update` function which modify the state after the database has been updated.

```rust
// Handle message from the tokio thread in
// a non-blocking way
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
```

Note that you will be defining your own AppMessage enum for all the possible actions you want to perform outside the main thread.

# Scaled Projects 

For larger projects, you would break the parts above into their own modules. I have a larger project in-progress right now and this is how I am currently doing it...

```
/src

  app.rs      <- module holding the application singleton and the
                 implementation of the eframe::App trait. In here
                 you'll create panels like egui::SidePanel,
                 egui::CentralPanel, etc... Inside the panels,
                 you will call to the ui elements provided in
                 the ui.rs module. Let's refer to the app
                 singleton as "App"

  main.rs     <- creates the eframe and application singleton
  
  /ui
  ui.rs       <- module to hold functions that accept
                 &mut Ui, &Context, or the mutable application
                 state (&mut self) or a subset of properties on
                 the state

  /database   
  database.rs <- module to hold database connection pool and
                 structs associated with the database models

  lib.rs      <- exposes the modules in the projects

  error.rs    <- module for application errors, conversions between
                 error types

  message_bus.rs  <- module to hold the messages that get passed
                     between the tokio thread and the main
                     egui thread (remember we can't block the
                     main thread). This also contains a function
                     called "handle_message" which accepts the
                     mutable state (&mut App) and listens for
                     messages from the tokio thread. This function
                     should be called from the update function
                     in app.rs.

```