mod app;
mod core;
mod ui;

fn main() -> std::io::Result<()> {
    app::r#loop::run()
}
