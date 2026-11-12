mod app;
mod editor;
use gpui::*;
fn main() {
    gpui_platform::application().run(|cx| {
        app::AppSession::bind_keys(cx);
        let home = std::env::var_os("HOME").expect("Application sandbox home is available");
        let documents = std::path::PathBuf::from(home).join("Documents");
        cx.open_window(WindowOptions::default(), |_, cx| {
            cx.new(|cx| app::AppSession::new(documents.join("notes"), cx))
        })
        .unwrap();
        cx.activate(true);
    });
}
