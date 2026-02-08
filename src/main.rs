use gtk::prelude::*;
use gtk::Application;

mod cava;
mod color_extractor;
mod lyrics;
mod mpd_client;
mod ui;
mod waveform;

use ui::MusicPlayerWindow;

const APP_ID: &str = "com.musicplayer.mpd";

fn main() {
    env_logger::init();
    
    let app = Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let window = MusicPlayerWindow::new(app);
    window.show();
}
