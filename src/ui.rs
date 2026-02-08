use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, DrawingArea, Image, Label,
    Orientation, Scale, ScrolledWindow, Stack, TextView, TreeView, ListStore, CellRendererText,
    CellRendererPixbuf, TreeViewColumn, SearchEntry, Revealer, RevealerTransitionType, Align, PolicyType,
};
use gdk;
use gdk_pixbuf::Pixbuf;
use glib;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use std::path::{Path, PathBuf};

use crate::cava::CavaVisualizer;
use crate::color_extractor::ColorExtractor;
use crate::lyrics::LRCParser;
use crate::mpd_client::{MPDClient, format_time};
use crate::waveform::{self, WaveformData, PeakPair};

use std::sync::{Arc, Mutex};

pub struct MusicPlayerWindow {
    window: ApplicationWindow,
    mpd: Rc<RefCell<MPDClient>>,
    
    // Background
    background: DrawingArea,
    // 4-corner palette for gradient background: [top-left, top-right, bottom-left, bottom-right]
    bg_palette: Rc<RefCell<[(f64, f64, f64); 4]>>,
    
    // Tabs
    player_tab: Button,
    library_tab: Button,
    stack: Stack,
    
    // Player view widgets
    album_art: Image,
    cava_area: DrawingArea,
    cava_bars: Arc<Mutex<Vec<u8>>>,
    song_title: Label,
    song_artist: Label,
    song_album: Label,
    time_label: Label,
    total_time_label: Label,
    waveform_area: DrawingArea,
    waveform_peaks: Rc<RefCell<Vec<PeakPair>>>,
    waveform_position: Rc<RefCell<f64>>,
    lyrics_scroll: ScrolledWindow,
    lyrics_box: GtkBox,
    play_btn: Button,
    prev_btn: Button,
    next_btn: Button,
    volume_scale: Scale,
    volume_percent: Label,
    queue_btn: Button,
    
    // Library view
    library_view: TreeView,
    library_store: ListStore,
    
    // Queue sidebar
    queue_revealer: Revealer,
    queue_view: TreeView,
    queue_store: ListStore,
    queue_filter: gtk::TreeModelFilter,
    queue_search: SearchEntry,
    queue_close_btn: Button,
    
    // State
    current_song_file: Rc<RefCell<String>>,
    current_lyrics: Rc<RefCell<Option<LRCParser>>>,
    current_lyrics_index: Rc<RefCell<Option<usize>>>,
    is_seeking: Rc<RefCell<bool>>,
    shuffle_enabled: Rc<RefCell<bool>>,
    repeat_enabled: Rc<RefCell<bool>>,
    // Album art cache: directory -> Option<art_path>
    art_cache: Rc<RefCell<HashMap<String, Option<String>>>>,
}

impl MusicPlayerWindow {
    pub fn new(app: &Application) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Bard")
            .default_width(380)
            .default_height(650)
            .build();

        let mpd = Rc::new(RefCell::new(
            MPDClient::new().expect("Failed to connect to MPD")
        ));

        // State
        // Initialize the background palette
        let bg_palette = Rc::new(RefCell::new([
            (0.08, 0.08, 0.10),
            (0.12, 0.10, 0.14),
            (0.10, 0.12, 0.08),
            (0.14, 0.10, 0.12),
        ]));
        let current_song_file = Rc::new(RefCell::new(String::new()));
        let current_lyrics = Rc::new(RefCell::new(None));
        let current_lyrics_index = Rc::new(RefCell::new(None));
        let waveform_peaks: Rc<RefCell<Vec<PeakPair>>> = Rc::new(RefCell::new(Vec::new()));
        let waveform_position: Rc<RefCell<f64>> = Rc::new(RefCell::new(0.0));
        let is_seeking = Rc::new(RefCell::new(false));
        let shuffle_enabled = Rc::new(RefCell::new(false));
        let repeat_enabled = Rc::new(RefCell::new(false));
        let art_cache: Rc<RefCell<HashMap<String, Option<String>>>> = Rc::new(RefCell::new(HashMap::new()));
        let bg_enabled: Rc<RefCell<bool>> = Rc::new(RefCell::new(true));

        // Create overlay for background
        let overlay = gtk::Overlay::new();
        
        // Background Drawing Area
        let background = DrawingArea::new();
        let bg_palette_clone = bg_palette.clone();

        // Pre-generate noise dither surface once (static across redraws)
        let noise_surface: Rc<RefCell<Option<cairo::ImageSurface>>> = Rc::new(RefCell::new(None));
        let noise_surface_clone = noise_surface.clone();

        // Cached gradient surface — only re-rendered when palette changes
        let gradient_cache: Rc<RefCell<Option<([(f64,f64,f64); 4], i32, i32, cairo::ImageSurface)>>> = Rc::new(RefCell::new(None));
        let gradient_cache_clone = gradient_cache.clone();

        // --- SMOOTH GRADIENT BACKGROUND (cached to ImageSurface) ---
        let bg_enabled_for_draw = bg_enabled.clone();
        background.connect_draw(move |widget, cr| {
            if !*bg_enabled_for_draw.borrow() {
                return glib::Propagation::Proceed;
            }
            let w = widget.allocated_width();
            let h = widget.allocated_height();
            let pal = *bg_palette_clone.borrow();

            // Check if we need to re-render the gradient surface
            let needs_render = {
                let gc = gradient_cache_clone.borrow();
                match gc.as_ref() {
                    Some((cached_pal, cw, ch, _)) => *cached_pal != pal || *cw != w || *ch != h,
                    None => true,
                }
            };

            if needs_render {
                if let Ok(surf) = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h) {
                    let cr2 = cairo::Context::new(&surf).unwrap();
                    let (tl_r, tl_g, tl_b) = pal[0];
                    let (tr_r, tr_g, tr_b) = pal[1];
                    let (bl_r, bl_g, bl_b) = pal[2];
                    let (br_r, br_g, br_b) = pal[3];

                    let mesh = cairo::Mesh::new();
                    mesh.begin_patch();
                    mesh.move_to(0.0, 0.0);
                    mesh.line_to(w as f64, 0.0);
                    mesh.line_to(w as f64, h as f64);
                    mesh.line_to(0.0, h as f64);
                    mesh.set_corner_color_rgb(cairo::MeshCorner::MeshCorner0, tl_r, tl_g, tl_b);
                    mesh.set_corner_color_rgb(cairo::MeshCorner::MeshCorner1, tr_r, tr_g, tr_b);
                    mesh.set_corner_color_rgb(cairo::MeshCorner::MeshCorner2, br_r, br_g, br_b);
                    mesh.set_corner_color_rgb(cairo::MeshCorner::MeshCorner3, bl_r, bl_g, bl_b);
                    mesh.end_patch();
                    cr2.set_source(&mesh).unwrap();
                    cr2.paint().unwrap();

                    // Bake noise dither into the cached surface too
                    {
                        let mut ns = noise_surface_clone.borrow_mut();
                        if ns.is_none() {
                            let tw: i32 = 128;
                            let th: i32 = 128;
                            if let Ok(mut nsurf) = cairo::ImageSurface::create(cairo::Format::ARgb32, tw, th) {
                                {
                                    let mut data = nsurf.data().unwrap();
                                    for y in 0..th {
                                        for x in 0..tw {
                                            let idx = ((y * tw + x) * 4) as usize;
                                            let mut hh = (x as u32).wrapping_mul(374761393)
                                                .wrapping_add((y as u32).wrapping_mul(668265263));
                                            hh = (hh ^ (hh >> 13)).wrapping_mul(1274126177);
                                            hh = hh ^ (hh >> 16);
                                            let v = (hh & 0xFF) as u8;
                                            let a: u8 = 3;
                                            let pv = ((v as u16 * a as u16) / 255) as u8;
                                            data[idx]     = pv;
                                            data[idx + 1] = pv;
                                            data[idx + 2] = pv;
                                            data[idx + 3] = a;
                                        }
                                    }
                                }
                                *ns = Some(nsurf);
                            }
                        }
                    }
                    if let Some(ref nsurf) = *noise_surface_clone.borrow() {
                        let pattern = cairo::SurfacePattern::create(nsurf);
                        pattern.set_extend(cairo::Extend::Repeat);
                        cr2.set_source(&pattern).unwrap();
                        cr2.paint().unwrap();
                    }

                    // Dark overlay
                    cr2.set_source_rgba(0.0, 0.0, 0.0, 0.10);
                    cr2.rectangle(0.0, 0.0, w as f64, h as f64);
                    cr2.fill().unwrap();

                    *gradient_cache_clone.borrow_mut() = Some((pal, w, h, surf));
                }
            }

            // Blit the cached surface (very fast — just a memcpy)
            if let Some((_, _, _, ref surf)) = *gradient_cache_clone.borrow() {
                cr.set_source_surface(surf, 0.0, 0.0).unwrap();
                cr.paint().unwrap();
            }

            glib::Propagation::Proceed
        });
        // ----------------------------
        
        overlay.add(&background);

        // Main container
        let main_box = GtkBox::new(Orientation::Vertical, 0);
        overlay.add_overlay(&main_box);

        // Top tabs
        let tabs_box = GtkBox::new(Orientation::Horizontal, 0);
        tabs_box.set_halign(Align::Center);
        tabs_box.set_margin_top(20);
        tabs_box.set_margin_bottom(10);

        let player_tab = Button::with_label("Now Playing");
        player_tab.set_widget_name("tab-button");
        player_tab.style_context().add_class("tab-button");
        player_tab.style_context().add_class("active");

        let library_tab = Button::with_label("Library");
        library_tab.set_widget_name("tab-button");
        library_tab.style_context().add_class("tab-button");

        tabs_box.pack_start(&player_tab, false, false, 0);
        tabs_box.pack_start(&library_tab, false, false, 0);
        main_box.pack_start(&tabs_box, false, false, 0);

        // Stack for player/library
        let stack = Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
        stack.set_transition_duration(150);

        // Create player view
        let (player_view, player_widgets) = Self::create_player_view();
        stack.add_named(&player_view, "player");

        // Create library view
        let (library_view_widget, library_view, library_store) = Self::create_library_view();
        stack.add_named(&library_view_widget, "library");

        // Queue button — pinned to absolute top-left of the window
        let queue_btn = Button::new();
        let queue_icon = load_icon_image(include_bytes!("assets/icons/view-queue-symbolic.svg"), 20, "#ffffff");
        queue_btn.set_image(Some(&queue_icon));
        queue_btn.set_always_show_image(true);
        queue_btn.style_context().add_class("control-button");
        queue_btn.style_context().add_class("icon-button");
        queue_btn.set_halign(Align::Start);
        queue_btn.set_valign(Align::Start);
        queue_btn.set_margin_start(10);
        queue_btn.set_margin_top(10);

        main_box.pack_start(&stack, true, true, 0);

        // Queue sidebar
        let queue_revealer = Revealer::new();
        queue_revealer.set_transition_type(RevealerTransitionType::SlideLeft);
        queue_revealer.set_transition_duration(150);
        queue_revealer.set_reveal_child(false);
        queue_revealer.set_halign(Align::End);
        queue_revealer.set_valign(Align::Fill);

        let (queue_box, queue_view, queue_store, queue_filter, queue_search, queue_close_btn) = Self::create_queue_sidebar();

        // Frosted-glass blur background for queue sidebar
        let queue_blur_cache: Rc<RefCell<Option<([(f64,f64,f64); 4], i32, i32, cairo::ImageSurface)>>> = Rc::new(RefCell::new(None));
        let queue_blur_clone = queue_blur_cache.clone();
        let gradient_cache_for_queue = gradient_cache.clone();
        let bg_palette_for_queue = bg_palette.clone();

        queue_box.connect_draw(move |widget, cr| {
            let w = widget.allocated_width();
            let h = widget.allocated_height();
            let pal = *bg_palette_for_queue.borrow();

            let needs_render = {
                let qbc = queue_blur_clone.borrow();
                match qbc.as_ref() {
                    Some((cp, cw, ch, _)) => *cp != pal || *cw != w || *ch != h,
                    None => true,
                }
            };

            if needs_render {
                let gc = gradient_cache_for_queue.borrow();
                if let Some((_, gw, _gh, ref grad_surf)) = *gc {
                    // Queue is right-aligned; sample the rightmost region
                    let x_off = (gw - w).max(0);
                    if let Ok(mut region) = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h) {
                        let rc = cairo::Context::new(&region).unwrap();
                        rc.set_source_surface(grad_surf, -(x_off as f64), 0.0).unwrap();
                        rc.paint().unwrap();
                        drop(rc);

                        if let Some(blurred) = blur_surface(&mut region, 18, 3) {
                            let bc = cairo::Context::new(&blurred).unwrap();
                            bc.set_source_rgba(0.0, 0.0, 0.0, 0.25);
                            bc.paint().unwrap();
                            drop(bc);
                            *queue_blur_clone.borrow_mut() = Some((pal, w, h, blurred));
                        }
                    }
                }
            }

            if let Some((_, _, _, ref blur_surf)) = *queue_blur_clone.borrow() {
                cr.set_source_surface(blur_surf, 0.0, 0.0).unwrap();
                cr.paint().unwrap();
            }

            glib::Propagation::Proceed
        });

        queue_revealer.add(&queue_box);
        
        overlay.add_overlay(&queue_revealer);
        overlay.add_overlay(&queue_btn);

        // Theme toggle button — top-right corner
        let theme_toggle = Button::new();
        theme_toggle.set_label("🎨");
        theme_toggle.style_context().add_class("control-button");
        theme_toggle.style_context().add_class("icon-button");
        theme_toggle.set_halign(Align::End);
        theme_toggle.set_valign(Align::Start);
        theme_toggle.set_margin_end(10);
        theme_toggle.set_margin_top(10);
        {
            let bg_enabled_toggle = bg_enabled.clone();
            let background_toggle = background.clone();
            let window_ref = window.clone();
            theme_toggle.connect_clicked(move |_btn| {
                let mut enabled = bg_enabled_toggle.borrow_mut();
                *enabled = !*enabled;
                if *enabled {
                    // Gradient mode: transparent window bg
                    window_ref.style_context().remove_class("gtk-theme");
                } else {
                    // GTK theme mode
                    window_ref.style_context().add_class("gtk-theme");
                }
                background_toggle.queue_draw();
            });
        }
        overlay.add_overlay(&theme_toggle);

        window.add(&overlay);

        // Clean stale art cache from old versions
        Self::clean_stale_art_cache();

        // Apply CSS
        Self::load_css();

        // Start CAVA visualizer (24 bars to fit album art height)
        let cava_num_bars: usize = 24;
        let cava_bars: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(vec![0u8; cava_num_bars]));
        // Keep the CAVA process alive for the lifetime of the window
        let _cava_process: Rc<RefCell<Option<CavaVisualizer>>> = Rc::new(RefCell::new(None));
        if let Some(cava) = CavaVisualizer::new(cava_num_bars) {
            let cava_bars_for_draw = cava.get_bars_arc();
            *_cava_process.borrow_mut() = Some(cava);
            // Set up CAVA draw callback with palette colors
            let bg_palette_for_cava = bg_palette.clone();
            player_widgets.1.connect_draw(move |widget, cr| {
                let w = widget.allocated_width() as f64;
                let h = widget.allocated_height() as f64;
                let bars = cava_bars_for_draw.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let pal = *bg_palette_for_cava.borrow();
                Self::draw_cava_bars(cr, &bars, w, h, &pal);
                glib::Propagation::Stop
            });
            // Redraw CAVA at ~30fps
            let cava_area_for_timer = player_widgets.1.clone();
            let _keep_cava_alive = _cava_process.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(33), move || {
                let _alive = &_keep_cava_alive; // prevent drop
                cava_area_for_timer.queue_draw();
                glib::ControlFlow::Continue
            });
        } else {
            // CAVA not available — hide the drawing area
            player_widgets.1.set_no_show_all(true);
            player_widgets.1.hide();
        }

        let mut player = Self {
            window,
            mpd,
            background,
            bg_palette,
            player_tab,
            library_tab,
            stack,
            album_art: player_widgets.0,
            cava_area: player_widgets.1,
            cava_bars,
            song_title: player_widgets.2,
            song_artist: player_widgets.3,
            song_album: player_widgets.4,
            time_label: player_widgets.5,
            total_time_label: player_widgets.6,
            waveform_area: player_widgets.7,
            waveform_peaks,
            waveform_position,
            lyrics_scroll: player_widgets.8,
            lyrics_box: player_widgets.9,
            play_btn: player_widgets.10,
            prev_btn: player_widgets.11,
            next_btn: player_widgets.12,
            volume_scale: player_widgets.13,
            volume_percent: player_widgets.14,
            queue_btn,
            library_view,
            library_store,
            queue_revealer,
            queue_view,
            queue_store,
            queue_filter,
            queue_search,
            queue_close_btn,
            current_song_file,
            current_lyrics,
            current_lyrics_index,
            is_seeking,
            shuffle_enabled,
            repeat_enabled,
            art_cache,
        };

        player.connect_signals();
        player.load_library_from_music();
        player.load_queue_from_mpd();
        player.precache_all_album_art();
        player.start_update_loop();

        player
    }

    /// Draw horizontal CAVA bars — each bar extends right-to-left based on amplitude.
    /// Bars are stacked vertically and colored using a vertical gradient from the album palette.
    /// `palette` is [top-left, top-right, bottom-left, bottom-right] RGB tuples.
    fn draw_cava_bars(cr: &cairo::Context, bars: &[u8], w: f64, h: f64, palette: &[(f64, f64, f64); 4]) {
        let num_bars = bars.len();
        if num_bars == 0 { return; }

        // Vertical gradient: interpolate left-side colors (top-left -> bottom-left)
        let (tl_r, tl_g, tl_b) = palette[0];
        let (bl_r, bl_g, bl_b) = palette[2];

        let gap = 2.0;
        let bar_height = (h - gap * (num_bars as f64 - 1.0)) / num_bars as f64;
        let corner_radius = bar_height * 0.3;

        for (i, &val) in bars.iter().enumerate() {
            let y = i as f64 * (bar_height + gap);
            let fraction = val as f64 / 255.0;
            let bar_width = (fraction * w).max(2.0);

            // Vertical interpolation factor for this bar
            let t = if num_bars > 1 { i as f64 / (num_bars - 1) as f64 } else { 0.5 };
            let r = tl_r + (bl_r - tl_r) * t;
            let g = tl_g + (bl_g - tl_g) * t;
            let b = tl_b + (bl_b - tl_b) * t;

            // Brighten the palette color and modulate alpha by amplitude
            let brighten = 1.6;
            let br = (r * brighten).min(1.0);
            let bg = (g * brighten).min(1.0);
            let bb = (b * brighten).min(1.0);
            let alpha = 0.4 + fraction * 0.6;
            cr.set_source_rgba(br, bg, bb, alpha);

            // Bars grow right-to-left (base against album art on the right)
            let x = w - bar_width;
            if bar_width > corner_radius * 2.0 {
                cr.new_path();
                cr.arc(x + corner_radius, y + corner_radius, corner_radius, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
                cr.arc(x + bar_width - corner_radius, y + corner_radius, corner_radius, 1.5 * std::f64::consts::PI, 0.0);
                cr.arc(x + bar_width - corner_radius, y + bar_height - corner_radius, corner_radius, 0.0, 0.5 * std::f64::consts::PI);
                cr.arc(x + corner_radius, y + bar_height - corner_radius, corner_radius, 0.5 * std::f64::consts::PI, std::f64::consts::PI);
                cr.close_path();
                cr.fill().unwrap();
            } else {
                cr.rectangle(x, y, bar_width, bar_height);
                cr.fill().unwrap();
            }
        }
    }

    fn create_player_view() -> (GtkBox, (Image, DrawingArea, Label, Label, Label, Label, Label, DrawingArea, ScrolledWindow, GtkBox, Button, Button, Button, Scale, Label)) {
        let player_box = GtkBox::new(Orientation::Vertical, 12);
        player_box.set_margin_start(20);
        player_box.set_margin_end(20);
        player_box.set_margin_top(15);
        player_box.set_margin_bottom(20);

        // Horizontal row: [cava bars | album art]
        let art_row = GtkBox::new(Orientation::Horizontal, 0);
        art_row.set_halign(Align::Center);

        // CAVA visualizer drawing area — outside, left of album art
        let cava_area = DrawingArea::new();
        cava_area.set_size_request(48, 210);
        cava_area.set_app_paintable(true);
        cava_area.style_context().add_class("cava-area");
        art_row.pack_start(&cava_area, false, false, 0);

        // Album art
        let album_art_frame = GtkBox::new(Orientation::Horizontal, 0);
        album_art_frame.set_halign(Align::Center);
        album_art_frame.set_size_request(210, 210);
        album_art_frame.style_context().add_class("album-art-frame");

        let album_art = Image::new();
        album_art.set_size_request(210, 210);
        album_art_frame.pack_start(&album_art, true, true, 0);

        art_row.pack_start(&album_art_frame, false, false, 0);

        // Invisible spacer to balance the CAVA area and keep art centered
        let right_spacer = DrawingArea::new();
        right_spacer.set_size_request(48, 1);
        art_row.pack_start(&right_spacer, false, false, 0);

        player_box.pack_start(&art_row, false, false, 0);

        // Song info
        let song_title = Label::new(Some("No song playing"));
        song_title.style_context().add_class("song-title");
        song_title.set_line_wrap(true);
        song_title.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        song_title.set_justify(gtk::Justification::Center);
        song_title.set_halign(Align::Center);
        player_box.pack_start(&song_title, false, false, 5);

        let song_artist = Label::new(Some(""));
        song_artist.style_context().add_class("song-artist");
        song_artist.set_line_wrap(true);
        song_artist.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        song_artist.set_justify(gtk::Justification::Center);
        song_artist.set_halign(Align::Center);
        player_box.pack_start(&song_artist, false, false, 0);

        let song_album = Label::new(Some(""));
        song_album.style_context().add_class("song-album");
        song_album.set_line_wrap(true);
        song_album.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        song_album.set_justify(gtk::Justification::Center);
        song_album.set_halign(Align::Center);
        player_box.pack_start(&song_album, false, false, 0);

        // Waveform progress bar
        let waveform_area = DrawingArea::new();
        waveform_area.set_size_request(280, 48);
        waveform_area.set_margin_top(16);
        waveform_area.style_context().add_class("waveform-area");
        // Events will be connected in connect_signals
        waveform_area.add_events(
            gdk::EventMask::BUTTON_PRESS_MASK
            | gdk::EventMask::BUTTON_RELEASE_MASK
            | gdk::EventMask::POINTER_MOTION_MASK
        );

        let progress_container = GtkBox::new(Orientation::Vertical, 4);
        progress_container.set_halign(Align::Center);
        progress_container.pack_start(&waveform_area, false, false, 0);
        
        // Time labels below waveform
        let time_box = GtkBox::new(Orientation::Horizontal, 0);
        time_box.set_halign(Align::Fill);
        time_box.set_size_request(280, -1);
        
        let time_label = Label::new(Some("0:00"));
        time_label.style_context().add_class("time-label");
        time_label.set_halign(Align::Start);
        time_box.pack_start(&time_label, true, true, 0);
        
        let total_time_label = Label::new(Some("-0:00"));
        total_time_label.style_context().add_class("time-label");
        total_time_label.set_halign(Align::End);
        time_box.pack_end(&total_time_label, true, true, 0);
        
        progress_container.pack_start(&time_box, false, false, 0);
        player_box.pack_start(&progress_container, false, false, 0);

        // Controls - compact and elegant
        let controls_box = GtkBox::new(Orientation::Horizontal, 20);
        controls_box.set_halign(Align::Center);
        controls_box.set_margin_top(12);

        let prev_btn = Button::new();
        let prev_icon = load_icon_image(include_bytes!("assets/icons/media-skip-backward-symbolic.svg"), 16, "#ffffff");
        prev_btn.set_image(Some(&prev_icon));
        prev_btn.set_always_show_image(true);
        prev_btn.style_context().add_class("control-button");
        prev_btn.style_context().add_class("small-control");
        controls_box.pack_start(&prev_btn, false, false, 0);

        let play_btn = Button::new();
        let play_icon = load_icon_image(include_bytes!("assets/icons/media-playback-start-symbolic.svg"), 22, "#ffffff");
        play_btn.set_image(Some(&play_icon));
        play_btn.set_always_show_image(true);
        play_btn.style_context().add_class("control-button");
        play_btn.style_context().add_class("play-button");
        controls_box.pack_start(&play_btn, false, false, 0);

        let next_btn = Button::new();
        let next_icon = load_icon_image(include_bytes!("assets/icons/media-skip-forward-symbolic.svg"), 16, "#ffffff");
        next_btn.set_image(Some(&next_icon));
        next_btn.set_always_show_image(true);
        next_btn.style_context().add_class("control-button");
        next_btn.style_context().add_class("small-control");
        controls_box.pack_start(&next_btn, false, false, 0);

        player_box.pack_start(&controls_box, false, false, 0);

        // Volume bar with speaker icons
        let volume_box = GtkBox::new(Orientation::Horizontal, 6);
        volume_box.set_halign(Align::Center);
        volume_box.set_margin_top(12);

        let vol_low_icon = load_icon_image(include_bytes!("assets/icons/audio-volume-low-symbolic.svg"), 14, "#ffffff");
        vol_low_icon.set_opacity(0.6);
        volume_box.pack_start(&vol_low_icon, false, false, 0);

        let volume_scale = Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 5.0);
        volume_scale.set_size_request(140, -1);
        volume_scale.set_draw_value(false);
        volume_scale.set_value(40.0);
        volume_scale.style_context().add_class("volume-scale");
        volume_box.pack_start(&volume_scale, false, false, 0);

        let vol_high_icon = load_icon_image(include_bytes!("assets/icons/audio-volume-high-symbolic.svg"), 14, "#ffffff");
        vol_high_icon.set_opacity(0.6);
        volume_box.pack_start(&vol_high_icon, false, false, 0);

        let volume_percent = Label::new(Some("40%"));
        volume_percent.set_visible(false);
        volume_percent.set_no_show_all(true);
        volume_box.pack_start(&volume_percent, false, false, 0);

        player_box.pack_start(&volume_box, false, false, 0);

        // Synced lyrics view — fills remaining space below controls
        let lyrics_scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        lyrics_scroll.set_policy(PolicyType::Never, PolicyType::External);
        lyrics_scroll.style_context().add_class("lyrics-scroll");

        let lyrics_box = GtkBox::new(Orientation::Vertical, 2);
        lyrics_box.set_halign(Align::Center);
        lyrics_box.set_valign(Align::Start);
        lyrics_box.set_margin_top(16);
        lyrics_box.set_margin_bottom(40);
        lyrics_box.set_margin_start(20);
        lyrics_box.set_margin_end(20);
        lyrics_scroll.add(&lyrics_box);
        lyrics_scroll.set_no_show_all(true);
        lyrics_scroll.hide();
        player_box.pack_start(&lyrics_scroll, true, true, 0);

        (player_box, (
            album_art,
            cava_area,
            song_title,
            song_artist,
            song_album,
            time_label,
            total_time_label,
            waveform_area,
            lyrics_scroll,
            lyrics_box,
            play_btn,
            prev_btn,
            next_btn,
            volume_scale,
            volume_percent,
        ))
    }

    fn create_library_view() -> (GtkBox, TreeView, ListStore) {
        // ... (unchanged)
        let library_box = GtkBox::new(Orientation::Vertical, 0);
        library_box.set_margin_start(20);
        library_box.set_margin_end(20);
        library_box.set_margin_top(20);
        library_box.set_margin_bottom(20);

        // Search
        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search your folders..."));
        library_box.pack_start(&search_entry, false, false, 10);

        // Library list
        let library_scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        library_scroll.set_policy(PolicyType::Never, PolicyType::Automatic);

        // Store: (folder_name, folder_path, play_button_visible)
        let library_store = ListStore::new(&[glib::Type::STRING, glib::Type::STRING, glib::Type::STRING]);
        let library_view = TreeView::with_model(&library_store);
        library_view.set_headers_visible(false);

        // Folder name column
        let renderer = CellRendererText::new();
        renderer.set_property("foreground", "#ffffff");
        let column = TreeViewColumn::new();
        column.set_title("Folder");
        gtk::prelude::CellLayoutExt::pack_start(&column, &renderer, true);
        gtk::prelude::CellLayoutExt::add_attribute(&column, &renderer, "text", 0);
        library_view.append_column(&column);

        library_scroll.add(&library_view);
        library_box.pack_start(&library_scroll, true, true, 0);

        (library_box, library_view, library_store)
    }

    fn create_queue_sidebar() -> (GtkBox, TreeView, ListStore, gtk::TreeModelFilter, SearchEntry, Button) {
        let queue_box = GtkBox::new(Orientation::Vertical, 0);
        queue_box.set_size_request(350, -1);
        queue_box.style_context().add_class("queue-sidebar");

        // Header
        let header = GtkBox::new(Orientation::Horizontal, 0);
        header.set_margin_start(15);
        header.set_margin_end(15);
        header.set_margin_top(15);
        header.set_margin_bottom(10);

        let queue_label = Label::new(Some("Playlist"));
        queue_label.set_markup("<span size='large' weight='bold' foreground='#ffffff'>Playlist</span>");
        header.pack_start(&queue_label, false, false, 0);

        let spacer = GtkBox::new(Orientation::Horizontal, 0);
        header.pack_start(&spacer, true, true, 0);

        let close_btn = Button::new();
        let close_icon = load_icon_image(include_bytes!("assets/icons/go-previous-symbolic.svg"), 18, "#ffffff");
        close_btn.set_image(Some(&close_icon));
        close_btn.set_always_show_image(true);
        close_btn.style_context().add_class("icon-button");
        header.pack_end(&close_btn, false, false, 0);

        queue_box.pack_start(&header, false, false, 0);

        // Search bar
        let search_entry = SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search queue..."));
        search_entry.style_context().add_class("queue-search");
        search_entry.set_margin_start(15);
        search_entry.set_margin_end(15);
        search_entry.set_margin_bottom(8);
        queue_box.pack_start(&search_entry, false, false, 0);

        // Queue list
        let queue_scroll = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        queue_scroll.set_policy(PolicyType::Never, PolicyType::Automatic);

        // Store: (title, artist, pixbuf, is_playing)
        let queue_store = ListStore::new(&[
            glib::Type::STRING,           // 0: title
            glib::Type::STRING,           // 1: artist
            gdk_pixbuf::Pixbuf::static_type(), // 2: album art thumbnail
            glib::Type::BOOL,             // 3: is_playing
        ]);
        // Wrap store in a filter model for search
        let queue_filter = gtk::TreeModelFilter::new(&queue_store, None);
        let search_entry_for_filter = search_entry.clone();
        queue_filter.set_visible_func(move |model, iter| {
            let query = search_entry_for_filter.text();
            let query = query.trim().to_lowercase();
            if query.is_empty() {
                return true;
            }
            let title = model.value(iter, 0).get::<String>().unwrap_or_default().to_lowercase();
            let artist = model.value(iter, 1).get::<String>().unwrap_or_default().to_lowercase();
            title.contains(&query) || artist.contains(&query)
        });

        let queue_view = TreeView::with_model(&queue_filter);
        queue_view.set_headers_visible(false);
        queue_view.set_activate_on_single_click(false);

        // Single column with art + text
        let column = TreeViewColumn::new();
        column.set_spacing(10);

        // Album art thumbnail renderer
        let art_renderer = CellRendererPixbuf::new();
        art_renderer.set_padding(8, 6);
        gtk::prelude::CellLayoutExt::pack_start(&column, &art_renderer, false);
        gtk::prelude::CellLayoutExt::add_attribute(&column, &art_renderer, "pixbuf", 2);

        // Title + Artist text renderer (using markup)
        let text_renderer = CellRendererText::new();
        text_renderer.set_property("ellipsize", gtk::pango::EllipsizeMode::End);
        gtk::prelude::CellLayoutExt::pack_start(&column, &text_renderer, true);

        // Use a cell data func to render title (bold) + artist (dim) as markup
        gtk::prelude::CellLayoutExt::set_cell_data_func(&column, &text_renderer, Some(Box::new(
            move |_col, cell, model, iter| {
                let title = model.value(iter, 0).get::<String>().unwrap_or_default();
                let artist = model.value(iter, 1).get::<String>().unwrap_or_default();
                let is_playing: bool = model.value(iter, 3).get::<bool>().unwrap_or(false);
                let title_color = if is_playing { "#ffffff" } else { "#dddddd" };
                let artist_color = if is_playing { "#aaaaaa" } else { "#888888" };
                let weight = if is_playing { "bold" } else { "semibold" };
                let markup = format!(
                    "<span foreground='{}' weight='{}' size='medium'>{}</span>\n<span foreground='{}' size='small'>{}</span>",
                    title_color, weight,
                    glib::markup_escape_text(&title),
                    artist_color,
                    glib::markup_escape_text(&artist)
                );
                cell.set_property("markup", &markup);
            }
        )));

        queue_view.append_column(&column);

        queue_scroll.add(&queue_view);
        queue_box.pack_start(&queue_scroll, true, true, 0);

        (queue_box, queue_view, queue_store, queue_filter, search_entry, close_btn)
    }

    fn connect_signals(&mut self) {
        // ... (unchanged signal connections for tabs, controls, seek, volume, queue)
        // Tab switching
        let stack_clone = self.stack.clone();
        let player_tab_clone = self.player_tab.clone();
        let library_tab_clone = self.library_tab.clone();
        
        self.player_tab.connect_clicked(move |_| {
            stack_clone.set_visible_child_name("player");
            player_tab_clone.style_context().add_class("active");
            library_tab_clone.style_context().remove_class("active");
        });

        let stack_clone = self.stack.clone();
        let player_tab_clone = self.player_tab.clone();
        let library_tab_clone = self.library_tab.clone();
        
        self.library_tab.connect_clicked(move |_| {
            stack_clone.set_visible_child_name("library");
            library_tab_clone.style_context().add_class("active");
            player_tab_clone.style_context().remove_class("active");
        });

        // Playback controls
        let mpd_clone = self.mpd.clone();
        self.play_btn.connect_clicked(move |_| {
            if let Ok(mut mpd) = mpd_clone.try_borrow_mut() {
                if let Ok(status) = mpd.status() {
                    match status.state {
                        mpd::State::Play => { let _ = mpd.pause(true); }
                        _ => { let _ = mpd.play(); }
                    }
                }
            }
        });

        let mpd_clone = self.mpd.clone();
        self.prev_btn.connect_clicked(move |_| {
            if let Ok(mut mpd) = mpd_clone.try_borrow_mut() {
                let _ = mpd.previous();
            }
        });

        let mpd_clone = self.mpd.clone();
        self.next_btn.connect_clicked(move |_| {
            if let Ok(mut mpd) = mpd_clone.try_borrow_mut() {
                let _ = mpd.next();
            }
        });

        // Waveform draw handler
        let wf_peaks = self.waveform_peaks.clone();
        let wf_pos = self.waveform_position.clone();
        self.waveform_area.connect_draw(move |_widget, cr| {
            let w = _widget.allocated_width() as f64;
            let h = _widget.allocated_height() as f64;
            let peaks = wf_peaks.borrow();
            let pos = *wf_pos.borrow();
            if peaks.is_empty() {
                waveform::draw_placeholder(cr, w, h);
            } else {
                waveform::draw_waveform(cr, &peaks, pos, w, h);
            }
            glib::Propagation::Proceed
        });

        // Seek via waveform click/drag
        let is_seeking_clone = self.is_seeking.clone();
        let wf_pos_for_press = self.waveform_position.clone();
        let wf_area_for_press = self.waveform_area.clone();
        self.waveform_area.connect_button_press_event(move |widget, event| {
            *is_seeking_clone.borrow_mut() = true;
            let w = widget.allocated_width() as f64;
            let pos = (event.position().0 / w).clamp(0.0, 1.0);
            *wf_pos_for_press.borrow_mut() = pos;
            wf_area_for_press.queue_draw();
            glib::Propagation::Proceed
        });

        let is_seeking_motion = self.is_seeking.clone();
        let wf_pos_for_motion = self.waveform_position.clone();
        let wf_area_for_motion = self.waveform_area.clone();
        self.waveform_area.connect_motion_notify_event(move |widget, event| {
            if *is_seeking_motion.borrow() {
                let w = widget.allocated_width() as f64;
                let pos = (event.position().0 / w).clamp(0.0, 1.0);
                *wf_pos_for_motion.borrow_mut() = pos;
                wf_area_for_motion.queue_draw();
            }
            glib::Propagation::Proceed
        });

        let is_seeking_clone = self.is_seeking.clone();
        let mpd_clone = self.mpd.clone();
        let wf_pos_for_release = self.waveform_position.clone();
        self.waveform_area.connect_button_release_event(move |_, _| {
            *is_seeking_clone.borrow_mut() = false;
            let pos = *wf_pos_for_release.borrow();
            if let Ok(mut mpd) = mpd_clone.try_borrow_mut() {
                if let Ok(status) = mpd.status() {
                    if let Some(duration) = status.duration {
                        let seek_time = pos * duration.as_secs_f64();
                        let _ = mpd.seek(Duration::from_secs_f64(seek_time));
                    }
                }
            }
            glib::Propagation::Proceed
        });

        // Volume
        let mpd_clone = self.mpd.clone();
        let volume_percent_clone = self.volume_percent.clone();
        
        // Handle scroll wheel - enforce strict 5% increments
        self.volume_scale.connect_scroll_event(move |scale, event| {
            let current = scale.value();
            
            // Snap the current value to nearest 5 before applying delta
            let snapped_base = (current / 5.0).round() * 5.0;

            let delta = match event.direction() {
                gdk::ScrollDirection::Up => 5.0,
                gdk::ScrollDirection::Down => -5.0,
                gdk::ScrollDirection::Smooth => {
                    let (_, dy) = event.scroll_deltas().unwrap_or((0.0, 0.0));
                    if dy < 0.0 { 5.0 } else if dy > 0.0 { -5.0 } else { 0.0 }
                }
                _ => return glib::Propagation::Proceed,
            };
            
            let new_value = (snapped_base + delta).clamp(0.0, 100.0);
            scale.set_value(new_value);
            
            glib::Propagation::Stop
        });
        
        self.volume_scale.connect_value_changed(move |scale| {
            let raw_value = scale.value();
            let snapped = (raw_value / 5.0).round() * 5.0;
            
            // Update widget and MPD if not already at snapped value
            if (raw_value - snapped).abs() > 0.01 {
                scale.set_value(snapped);
            } else {
                let volume_int = snapped as i8;
                volume_percent_clone.set_text(&format!("{}%", volume_int));
                if let Ok(mut mpd) = mpd_clone.try_borrow_mut() {
                    let _ = mpd.set_volume(volume_int);
                }
            }
        });
        
        // Queue toggle
        let queue_revealer_clone = self.queue_revealer.clone();
        self.queue_btn.connect_clicked(move |btn| {
            let revealed = queue_revealer_clone.reveals_child();
            queue_revealer_clone.set_reveal_child(!revealed);
            
            if !revealed {
                btn.style_context().add_class("active");
            } else {
                btn.style_context().remove_class("active");
            }
        });

        // Queue close button
        let queue_revealer_clone = self.queue_revealer.clone();
        let queue_btn_clone = self.queue_btn.clone();
        self.queue_close_btn.connect_clicked(move |_| {
            queue_revealer_clone.set_reveal_child(false);
            queue_btn_clone.style_context().remove_class("active");
        });

        // Queue search: refilter on text change
        let queue_filter_clone = self.queue_filter.clone();
        self.queue_search.connect_search_changed(move |_| {
            queue_filter_clone.refilter();
        });

        // Queue song double-click to play (map filter path → store path for real queue position)
        let mpd_clone = self.mpd.clone();
        let queue_filter_for_activate = self.queue_filter.clone();
        let queue_search_for_activate = self.queue_search.clone();
        self.queue_view.connect_row_activated(move |_, path, _| {
            // Convert filter path to underlying store path
            if let Some(store_path) = queue_filter_for_activate.convert_path_to_child_path(path) {
                let indices = store_path.indices();
                if let Some(&pos) = indices.first() {
                    if let Ok(mut mpd) = mpd_clone.try_borrow_mut() {
                        let _ = mpd.play_pos(pos as u32);
                    }
                }
            }
            // Clear search bar after selection
            queue_search_for_activate.set_text("");
        });

        // Library folder playback
        let _library_view_clone = self.library_view.clone();
        let library_store_clone = self.library_store.clone();
        let mpd_clone = self.mpd.clone();
        let queue_store_clone = self.queue_store.clone();
        
        self.library_view.connect_row_activated(move |_, path, _| {
            if let Some(iter) = library_store_clone.iter(path) {
                let folder_path_val = library_store_clone.value(&iter, 1);
                if let Ok(folder_path) = folder_path_val.get::<String>() {
                    Self::play_folder(&mpd_clone, &queue_store_clone, &folder_path);
                }
            }
        });
    }

    fn play_folder(mpd: &Rc<RefCell<MPDClient>>, queue_store: &ListStore, folder_path: &str) {
        use std::process::Command;

        if let Ok(mut mpd_client) = mpd.try_borrow_mut() {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let music_dir = PathBuf::from(&home).join("Music");
            
            // Get relative path from music directory
            let relative_folder = PathBuf::from(folder_path)
                .strip_prefix(&music_dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let _ = mpd_client.clear();

            // Add entire folder in one mpc call (much faster than per-song)
            let _ = Command::new("mpc")
                .args(&["-h", "127.0.0.1", "add", &relative_folder])
                .output();

            let _ = mpd_client.shuffle();
            let _ = mpd_client.play();

            queue_store.clear();
            let files: Vec<(String, String, String)> = if let Ok(songs) = mpd_client.get_queue() {
                songs.iter().map(|s| (
                    s.title.as_deref().unwrap_or("Unknown").to_string(),
                    s.artist.as_deref().unwrap_or("Unknown").to_string(),
                    s.file.clone(),
                )).collect()
            } else { vec![] };

            // Fast: populate text only
            for (title, artist, _) in &files {
                let iter = queue_store.append();
                queue_store.set_value(&iter, 0, &title.to_value());
                queue_store.set_value(&iter, 1, &artist.to_value());
                queue_store.set_value(&iter, 3, &false.to_value());
            }

            // Lazy art loading via idle
            let store = queue_store.clone();
            let file_list: Vec<String> = files.into_iter().map(|(_, _, f)| f).collect();
            let idx = Rc::new(RefCell::new(0usize));
            let cache: Rc<RefCell<HashMap<String, Option<String>>>> = Rc::new(RefCell::new(HashMap::new()));
            glib::idle_add_local(move || {
                let i = *idx.borrow();
                if i >= file_list.len() {
                    return glib::ControlFlow::Break;
                }
                if let Some(iter) = store.iter_nth_child(None, i as i32) {
                    if let Some(art_path) = Self::find_album_art_cached(&file_list[i], &cache) {
                        if let Ok(pb) = Pixbuf::from_file_at_scale(&art_path, 45, 45, true) {
                            store.set_value(&iter, 2, &pb.to_value());
                        }
                    }
                }
                *idx.borrow_mut() = i + 1;
                glib::ControlFlow::Continue
            });
        }
    }

    fn load_library_from_music(&self) {
        // ... (unchanged)
        use std::fs;
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let music_path = PathBuf::from(home).join("Music");
        
        if !music_path.exists() { return; }

        if let Ok(entries) = fs::read_dir(&music_path) {
            let mut folders: Vec<_> = entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_dir())
                .collect();
            
            folders.sort_by_key(|entry| entry.file_name());
            
            for entry in folders {
                let path = entry.path();
                if let Some(folder_name) = path.file_name() {
                    if let Some(name) = folder_name.to_str() {
                        let iter = self.library_store.append();
                        self.library_store.set(&iter, &[
                            (0, &name.to_value()),
                            (1, &path.to_string_lossy().to_value()),
                            (2, &"▶".to_value())
                        ]);
                    }
                }
            }
        }
    }

    fn load_queue_from_mpd(&self) {
        let files: Vec<(String, String, String)>;
        if let Ok(mut mpd) = self.mpd.try_borrow_mut() {
            if let Ok(songs) = mpd.get_queue() {
                files = songs.iter().map(|s| (
                    s.title.as_deref().unwrap_or("Unknown").to_string(),
                    s.artist.as_deref().unwrap_or("Unknown").to_string(),
                    s.file.clone(),
                )).collect();
            } else { return; }
        } else { return; }

        // Populate queue instantly with text only (no art = fast)
        for (title, artist, _) in &files {
            let iter = self.queue_store.append();
            self.queue_store.set_value(&iter, 0, &title.to_value());
            self.queue_store.set_value(&iter, 1, &artist.to_value());
            self.queue_store.set_value(&iter, 3, &false.to_value());
        }

        // Load art thumbnails lazily — one every 32ms to keep the UI responsive
        let store = self.queue_store.clone();
        let cache = self.art_cache.clone();
        let file_list: Vec<String> = files.into_iter().map(|(_, _, f)| f).collect();
        let idx = Rc::new(RefCell::new(0usize));
        glib::timeout_add_local(std::time::Duration::from_millis(32), move || {
            let i = *idx.borrow();
            if i >= file_list.len() {
                return glib::ControlFlow::Break;
            }
            if let Some(iter) = store.iter_nth_child(None, i as i32) {
                if let Some(art_path) = Self::find_album_art_cached(&file_list[i], &cache) {
                    if let Ok(pb) = Pixbuf::from_file_at_scale(&art_path, 45, 45, true) {
                        store.set_value(&iter, 2, &pb.to_value());
                    }
                }
            }
            *idx.borrow_mut() = i + 1;
            glib::ControlFlow::Continue
        });
    }

    /// Pre-cache album art for every audio file in ~/Music in the background.
    fn precache_all_album_art(&self) {
        use std::fs;
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return,
        };
        let music_path = PathBuf::from(&home).join("Music");
        if !music_path.exists() { return; }

        // Recursively collect all audio files
        let mut songs_to_cache: Vec<String> = Vec::new();
        let mut dirs = vec![music_path.clone()];
        while let Some(dir) = dirs.pop() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        dirs.push(path);
                    } else {
                        let fname = entry.file_name().to_string_lossy().to_lowercase();
                        if fname.ends_with(".mp3") || fname.ends_with(".flac")
                            || fname.ends_with(".ogg") || fname.ends_with(".opus")
                            || fname.ends_with(".m4a") || fname.ends_with(".wav") {
                            if let Ok(rel) = path.strip_prefix(&music_path) {
                                let rel_str = rel.to_string_lossy().to_string();
                                // Skip if already cached on disk
                                let safe = rel_str.replace('/', "_").replace(' ', "_");
                                let cached = Self::cache_dir().join(format!("{}.jpg", safe));
                                if !cached.exists() {
                                    songs_to_cache.push(rel_str);
                                }
                            }
                        }
                    }
                }
            }
        }

        if songs_to_cache.is_empty() { return; }

        let cache = self.art_cache.clone();
        let idx = Rc::new(RefCell::new(0usize));
        // Precache slowly — one file every 80ms so the UI stays smooth
        glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
            let i = *idx.borrow();
            if i >= songs_to_cache.len() {
                return glib::ControlFlow::Break;
            }
            let _ = Self::find_album_art_cached(&songs_to_cache[i], &cache);
            *idx.borrow_mut() = i + 1;
            glib::ControlFlow::Continue
        });
    }

    fn start_update_loop(&self) {
        let mpd_clone = self.mpd.clone();
        let song_title_clone = self.song_title.clone();
        let song_artist_clone = self.song_artist.clone();
        let song_album_clone = self.song_album.clone();
        let time_label_clone = self.time_label.clone();
        let total_time_label_clone = self.total_time_label.clone();
        let wf_pos_clone = self.waveform_position.clone();
        let wf_area_clone = self.waveform_area.clone();
        let wf_peaks_for_loop = self.waveform_peaks.clone();
        let play_btn_clone = self.play_btn.clone();
        let is_seeking_clone = self.is_seeking.clone();
        let current_song_file_clone = self.current_song_file.clone();
        let album_art_clone = self.album_art.clone();
        // Color extraction for gradient background
        let bg_palette_clone = self.bg_palette.clone();
        let background_clone = self.background.clone();
        let queue_store_clone = self.queue_store.clone();
        let queue_view_clone = self.queue_view.clone();
        let last_queue_pos: Rc<RefCell<Option<i32>>> = Rc::new(RefCell::new(None));
        let current_lyrics_clone = self.current_lyrics.clone();
        let current_lyrics_index_clone = self.current_lyrics_index.clone();
        let lyrics_box_clone = self.lyrics_box.clone();
        let lyrics_scroll_clone = self.lyrics_scroll.clone();
        let lyrics_scroll_target: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
        let lyrics_scroll_target_clone = lyrics_scroll_target.clone();
        let lyrics_scroll_for_anim = self.lyrics_scroll.clone();

        // Pre-render play/pause icon pixbufs once (avoid re-parsing SVG every 500ms)
        let play_pixbuf = load_icon_pixbuf(include_bytes!("assets/icons/media-playback-start-symbolic.svg"), 24, "#ffffff");
        let pause_pixbuf = load_icon_pixbuf(include_bytes!("assets/icons/media-playback-pause-symbolic.svg"), 24, "#ffffff");
        let play_pixbuf_rc = Rc::new(play_pixbuf);
        let pause_pixbuf_rc = Rc::new(pause_pixbuf);
        let play_pb_clone = play_pixbuf_rc.clone();
        let pause_pb_clone = pause_pixbuf_rc.clone();
        let last_play_state: Rc<RefCell<Option<bool>>> = Rc::new(RefCell::new(None));

        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            if let Ok(mut mpd) = mpd_clone.try_borrow_mut() {
                let status = mpd.status().ok();
                
                if let Some(ref status) = status {
                    let is_playing = matches!(status.state, mpd::State::Play);
                    let mut last_st = last_play_state.borrow_mut();
                    if *last_st != Some(is_playing) {
                        *last_st = Some(is_playing);
                        if is_playing {
                            if let Some(ref pb) = *pause_pb_clone {
                                play_btn_clone.set_image(Some(&Image::from_pixbuf(Some(pb))));
                            }
                        } else {
                            if let Some(ref pb) = *play_pb_clone {
                                play_btn_clone.set_image(Some(&Image::from_pixbuf(Some(pb))));
                            }
                        }
                    }

                    if let (Some(elapsed), Some(duration)) = (status.elapsed, status.duration) {
                        let current = elapsed.as_secs_f64();
                        let total = duration.as_secs_f64();

                        time_label_clone.set_text(&format_time(current));
                        let remaining = total - current;
                        total_time_label_clone.set_text(&format!("-{}", format_time(remaining)));

                        if !*is_seeking_clone.borrow() && total > 0.0 {
                            *wf_pos_clone.borrow_mut() = current / total;
                            wf_area_clone.queue_draw();
                        }

                        // Sync lyrics highlight
                        if let Some(ref lrc) = *current_lyrics_clone.borrow() {
                            if let Some((idx, _text)) = lrc.get_current_line(current) {
                                let mut last_idx = current_lyrics_index_clone.borrow_mut();
                                if *last_idx != Some(idx) {
                                    // Un-highlight old line — remove bold
                                    if let Some(old_idx) = *last_idx {
                                        if let Some(child) = lyrics_box_clone.children().get(old_idx) {
                                            child.style_context().remove_class("lyrics-active");
                                            child.style_context().add_class("lyrics-dim");
                                            if let Some(lbl) = child.downcast_ref::<Label>() {
                                                if let Some(ref line) = lrc.lines.get(old_idx) {
                                                    let escaped = glib::markup_escape_text(&line.text);
                                                    lbl.set_markup(&format!("<span size='medium'>{}</span>", escaped));
                                                }
                                            }
                                        }
                                    }
                                    // Highlight new line — set bold
                                    let children = lyrics_box_clone.children();
                                    if let Some(child) = children.get(idx) {
                                        child.style_context().remove_class("lyrics-dim");
                                        child.style_context().add_class("lyrics-active");
                                        if let Some(lbl) = child.downcast_ref::<Label>() {
                                            if let Some(ref line) = lrc.lines.get(idx) {
                                                let escaped = glib::markup_escape_text(&line.text);
                                                lbl.set_markup(&format!("<span size='medium' weight='bold'>{}</span>", escaped));
                                            }
                                        }
                                        // Smooth scroll — set target and start animation
                                        let alloc = child.allocation();
                                        let scroll_h = lyrics_scroll_clone.allocated_height() as f64;
                                        let target = (alloc.y() as f64) - (scroll_h / 2.0) + (alloc.height() as f64 / 2.0);
                                        *lyrics_scroll_target_clone.borrow_mut() = Some(target.max(0.0));
                                        let scroll_anim = lyrics_scroll_for_anim.clone();
                                        let target_anim = lyrics_scroll_target.clone();
                                        glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
                                            let adj = scroll_anim.vadjustment();
                                            let target_val = match *target_anim.borrow() {
                                                Some(t) => t,
                                                None => return glib::ControlFlow::Break,
                                            };
                                            let cur = adj.value();
                                            let diff = target_val - cur;
                                            if diff.abs() < 1.0 {
                                                adj.set_value(target_val);
                                                *target_anim.borrow_mut() = None;
                                                return glib::ControlFlow::Break;
                                            }
                                            // Ease toward target (lerp 15% per frame)
                                            adj.set_value(cur + diff * 0.15);
                                            glib::ControlFlow::Continue
                                        });
                                    }
                                    *last_idx = Some(idx);
                                }
                            }
                        }
                    }

                    // Track current queue position and highlight it
                    if let Some(mpd::song::QueuePlace { pos, .. }) = status.song {
                        let new_pos = pos as i32;
                        let mut last_pos = last_queue_pos.borrow_mut();
                        if *last_pos != Some(new_pos) {
                            // Only update the old and new rows (O(1) not O(n))
                            if let Some(old) = *last_pos {
                                if let Some(iter) = queue_store_clone.iter_nth_child(None, old) {
                                    queue_store_clone.set_value(&iter, 3, &false.to_value());
                                }
                            }
                            if let Some(iter) = queue_store_clone.iter_nth_child(None, new_pos) {
                                queue_store_clone.set_value(&iter, 3, &true.to_value());
                            }
                            // Auto-scroll to current song (only when not searching)
                            let store_path = gtk::TreePath::from_indicesv(&[new_pos]);
                            // The view uses the filter model, so we try to get a visible path
                            if let Some(model) = queue_view_clone.model() {
                                if let Some(filter) = model.dynamic_cast_ref::<gtk::TreeModelFilter>() {
                                    if let Some(filter_path) = filter.convert_child_path_to_path(&store_path) {
                                        queue_view_clone.scroll_to_cell(
                                            Some(&filter_path), None::<&TreeViewColumn>, true, 0.5, 0.0
                                        );
                                        queue_view_clone.selection().select_path(&filter_path);
                                    }
                                }
                            }
                            *last_pos = Some(new_pos);
                        }
                    }
                }

                if let Ok(Some(song)) = mpd.current_song() {
                    let file = song.file.clone();
                    
                    if file != *current_song_file_clone.borrow() {
                        *current_song_file_clone.borrow_mut() = file.clone();
                        
                        let title = song.title.as_deref().unwrap_or("Unknown");
                        let artist = song.artist.as_deref().unwrap_or("Unknown Artist");
                        let album = song.tags.iter().find(|(k, _)| k == "Album").map(|(_, v)| v.as_str());

                        song_title_clone.set_text(title);
                        song_artist_clone.set_text(artist);
                        if let Some(album) = album {
                            song_album_clone.set_text(album);
                            song_album_clone.show();
                        } else {
                            song_album_clone.set_text("");
                            song_album_clone.hide();
                        }

                        // Extract waveform peaks in background thread
                        {
                            let wf_peaks = wf_peaks_for_loop.clone();
                            let wf_area = wf_area_clone.clone();
                            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                            let full_path = PathBuf::from(&home).join("Music").join(&file);
                            let full_path_str = full_path.to_string_lossy().to_string();
                            // Clear current peaks immediately
                            wf_peaks.borrow_mut().clear();
                            wf_area.queue_draw();
                            // Use a channel to send peaks back to main thread
                            let (tx, rx) = std::sync::mpsc::channel::<Vec<PeakPair>>();
                            let wf_peaks_rx = wf_peaks.clone();
                            let wf_area_rx = wf_area.clone();
                            glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                                match rx.try_recv() {
                                    Ok(peaks) => {
                                        *wf_peaks_rx.borrow_mut() = peaks;
                                        wf_area_rx.queue_draw();
                                        glib::ControlFlow::Break
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                                    Err(_) => glib::ControlFlow::Break,
                                }
                            });
                            std::thread::spawn(move || {
                                // Target ~70 bars for a 280px wide area (bar=2px + gap=2px)
                                if let Some(data) = WaveformData::from_file(&full_path_str, 70) {
                                    let _ = tx.send(data.peaks);
                                }
                            });
                        }

                        if let Some(art_path) = Self::find_album_art(&file) {
                            let art_path_owned = art_path.clone();
                            let album_art_c = album_art_clone.clone();
                            let bg_palette_c = bg_palette_clone.clone();
                            let background_c = background_clone.clone();
                            // Defer heavy image load + palette extraction to an idle callback
                            // so it doesn't block the timer return and freeze the UI.
                            glib::idle_add_local_once(move || {
                                if let Ok(pixbuf) = Pixbuf::from_file_at_scale(&art_path_owned, 260, 260, true) {
                                    album_art_c.set_from_pixbuf(Some(&pixbuf));
                                    
                                    if let Some(palette) = ColorExtractor::extract_palette(&art_path_owned) {
                                        *bg_palette_c.borrow_mut() = [
                                            (palette[0].r, palette[0].g, palette[0].b),
                                            (palette[1].r, palette[1].g, palette[1].b),
                                            (palette[2].r, palette[2].g, palette[2].b),
                                            (palette[3].r, palette[3].g, palette[3].b),
                                        ];
                                    }
                                    background_c.queue_draw();
                                }
                            });
                        }

                        // Load synced lyrics from ~/Music/Lyrics/
                        {
                            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                            let lyrics_dir = PathBuf::from(&home).join("Music").join("Lyrics");
                            // Try "Artist - Title.lrc"
                            let lrc_path = lyrics_dir.join(format!("{} - {}.lrc", artist, title));
                            // Clear old lyrics
                            for child in lyrics_box_clone.children() {
                                lyrics_box_clone.remove(&child);
                            }
                            *current_lyrics_clone.borrow_mut() = None;
                            *current_lyrics_index_clone.borrow_mut() = None;
                            lyrics_scroll_clone.hide();

                            if lrc_path.exists() {
                                if let Some(lrc) = LRCParser::from_file(&lrc_path) {
                                    for (i, line) in lrc.lines.iter().enumerate() {
                                        let label = Label::new(None);
                                        let escaped = glib::markup_escape_text(&line.text);
                                        if line.text.is_empty() {
                                            label.set_markup("<span size='small'> </span>");
                                        } else {
                                            label.set_markup(&format!(
                                                "<span size='medium'>{}</span>", escaped
                                            ));
                                        }
                                        label.set_line_wrap(true);
                                        label.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
                                        label.set_justify(gtk::Justification::Center);
                                        label.set_halign(Align::Center);
                                        label.set_margin_top(4);
                                        label.set_margin_bottom(4);
                                        label.style_context().add_class("lyrics-dim");
                                        if i == 0 {
                                            label.style_context().remove_class("lyrics-dim");
                                            label.style_context().add_class("lyrics-active");
                                        }
                                        lyrics_box_clone.pack_start(&label, false, false, 0);
                                    }
                                    lyrics_scroll_clone.show();
                                    lyrics_box_clone.show_all();
                                    *current_lyrics_clone.borrow_mut() = Some(lrc);
                                }
                            }
                        }
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    /// Returns the cache directory path: ~/.cache/ArcanistPlayer/
    fn cache_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".cache").join("ArcanistPlayer")
    }

    /// Cached album art lookup — keyed per song file, result cached in-memory + on disk
    fn find_album_art_cached(song_path: &str, cache: &Rc<RefCell<HashMap<String, Option<String>>>>) -> Option<String> {
        // Check in-memory cache first (keyed by relative song path)
        if let Some(cached) = cache.borrow().get(song_path) {
            return cached.clone();
        }

        let result = Self::resolve_album_art(song_path);
        cache.borrow_mut().insert(song_path.to_string(), result.clone());
        result
    }

    fn find_album_art(song_path: &str) -> Option<String> {
        Self::resolve_album_art(song_path)
    }

    /// The single source of truth for album art resolution.
    /// Priority: disk cache → folder art files → embedded art (extract + cache to disk)
    fn resolve_album_art(song_path: &str) -> Option<String> {
        let home = std::env::var("HOME").ok()?;
        let music_dir = PathBuf::from(&home).join("Music");
        let song_full_path = music_dir.join(song_path);
        let cache_dir = Self::cache_dir();

        // Deterministic cache filename from the song's relative path
        let safe_name = song_path.replace('/', "_").replace(' ', "_");
        let disk_cache_path = cache_dir.join(format!("{}.jpg", safe_name));

        // 1) Check on-disk cache
        if disk_cache_path.exists() {
            return disk_cache_path.to_str().map(|s| s.to_string());
        }

        // 2) Check loose art files in the song's directory
        if let Some(song_dir) = song_full_path.parent() {
            let art_names = ["cover.jpg", "cover.png", "folder.jpg", "folder.png", "albumart.jpg", "albumart.png"];
            for name in &art_names {
                let art_path = song_dir.join(name);
                if art_path.exists() {
                    let _ = std::fs::create_dir_all(&cache_dir);
                    let _ = std::fs::copy(&art_path, &disk_cache_path);
                    return disk_cache_path.to_str().map(|s| s.to_string());
                }
            }
        }

        // 3) Extract embedded art and write to disk cache
        let _ = std::fs::create_dir_all(&cache_dir);
        Self::extract_embedded_to_cache(&song_full_path, &disk_cache_path)
    }

    fn extract_embedded_to_cache(song_path: &Path, cache_path: &Path) -> Option<String> {
        let path_str = song_path.to_str()?;
        let lower = path_str.to_lowercase();
        if lower.ends_with(".mp3") {
            use id3::Tag;
            if let Ok(tag) = Tag::read_from_path(song_path) {
                for frame in tag.frames() {
                    if let id3::frame::Content::Picture(pic) = frame.content() {
                        if std::fs::write(cache_path, &pic.data).is_ok() {
                            return cache_path.to_str().map(|s| s.to_string());
                        }
                    }
                }
            }
        } else if lower.ends_with(".flac") {
            use metaflac::Tag;
            if let Ok(tag) = Tag::read_from_path(song_path) {
                for picture in tag.pictures() {
                    if std::fs::write(cache_path, &picture.data).is_ok() {
                        return cache_path.to_str().map(|s| s.to_string());
                    }
                }
            }
        }
        None
    }

    fn load_css() {
        // ... (unchanged)
        let css_provider = gtk::CssProvider::new();
        let css = include_str!("../style.css");
        let _ = css_provider.load_from_data(css.as_bytes());
        gtk::StyleContext::add_provider_for_screen(
            &gdk::Screen::default().unwrap(),
            &css_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    pub fn show(&self) {
        self.window.show_all();
    }

    /// Remove old-style temp art files and ensure cache directory exists
    fn clean_stale_art_cache() {
        // Clean old PID-based and hash-based stale files from /tmp
        if let Ok(entries) = std::fs::read_dir("/tmp") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if (name.starts_with("mpd_album_") || name.starts_with("mpd_art_")) && name.ends_with(".jpg") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        // Clean old /tmp/Album_art if it exists
        let _ = std::fs::remove_dir_all("/tmp/Album_art");
        // Ensure new cache directory exists
        let _ = std::fs::create_dir_all(Self::cache_dir());
    }
}

/// Apply multi-pass box blur to a Cairo ImageSurface, returning a new blurred surface.
fn blur_surface(surf: &mut cairo::ImageSurface, radius: i32, passes: u32) -> Option<cairo::ImageSurface> {
    let w = surf.width();
    let h = surf.height();
    if w == 0 || h == 0 { return None; }
    let stride = surf.stride() as usize;

    let src_data = surf.data().ok()?;
    let mut buf_a = src_data.to_vec();
    drop(src_data);
    let mut buf_b = vec![0u8; buf_a.len()];

    for _ in 0..passes {
        // Horizontal pass: buf_a -> buf_b
        for y in 0..h as usize {
            for x in 0..w as usize {
                let mut sums = [0u32; 4];
                let mut count = 0u32;
                let x_min = (x as i32 - radius).max(0) as usize;
                let x_max = (x as i32 + radius).min(w - 1) as usize;
                for sx in x_min..=x_max {
                    let idx = y * stride + sx * 4;
                    sums[0] += buf_a[idx] as u32;
                    sums[1] += buf_a[idx + 1] as u32;
                    sums[2] += buf_a[idx + 2] as u32;
                    sums[3] += buf_a[idx + 3] as u32;
                    count += 1;
                }
                let idx = y * stride + x * 4;
                buf_b[idx]     = (sums[0] / count) as u8;
                buf_b[idx + 1] = (sums[1] / count) as u8;
                buf_b[idx + 2] = (sums[2] / count) as u8;
                buf_b[idx + 3] = (sums[3] / count) as u8;
            }
        }
        // Vertical pass: buf_b -> buf_a
        for y in 0..h as usize {
            for x in 0..w as usize {
                let mut sums = [0u32; 4];
                let mut count = 0u32;
                let y_min = (y as i32 - radius).max(0) as usize;
                let y_max = (y as i32 + radius).min(h - 1) as usize;
                for sy in y_min..=y_max {
                    let idx = sy * stride + x * 4;
                    sums[0] += buf_b[idx] as u32;
                    sums[1] += buf_b[idx + 1] as u32;
                    sums[2] += buf_b[idx + 2] as u32;
                    sums[3] += buf_b[idx + 3] as u32;
                    count += 1;
                }
                let idx = y * stride + x * 4;
                buf_a[idx]     = (sums[0] / count) as u8;
                buf_a[idx + 1] = (sums[1] / count) as u8;
                buf_a[idx + 2] = (sums[2] / count) as u8;
                buf_a[idx + 3] = (sums[3] / count) as u8;
            }
        }
    }

    let mut out = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).ok()?;
    {
        let mut data = out.data().ok()?;
        data[..buf_a.len()].copy_from_slice(&buf_a);
    }
    Some(out)
}

/// Load an SVG icon from embedded bytes, recolour it, render at `size` px, and return a GTK Image.
/// `color` is a CSS hex colour like "#ffffff".
fn load_icon_image(svg_bytes: &[u8], size: i32, color: &str) -> Image {
    if let Some(pixbuf) = load_icon_pixbuf(svg_bytes, size, color) {
        Image::from_pixbuf(Some(&pixbuf))
    } else {
        Image::new()
    }
}

/// Load an SVG icon from embedded bytes, recolour and render at `size` px, return the Pixbuf.
fn load_icon_pixbuf(svg_bytes: &[u8], size: i32, color: &str) -> Option<Pixbuf> {
    let svg_str = String::from_utf8_lossy(svg_bytes);
    let recolored = svg_str
        .replace("fill=\"#2e3436\"", &format!("fill=\"{}\"", color))
        .replace("fill=\"#000000\"", &format!("fill=\"{}\"", color))
        .replace("fill=\"black\"", &format!("fill=\"{}\"", color))
        .replace("stroke=\"#2e3436\"", &format!("stroke=\"{}\"", color))
        .replace("stroke=\"#000000\"", &format!("stroke=\"{}\"", color))
        .replace("stroke=\"black\"", &format!("stroke=\"{}\"", color));
    let loader = gdk_pixbuf::PixbufLoader::with_type("svg").ok()?;
    loader.set_size(size, size);
    loader.write(recolored.as_bytes()).ok()?;
    loader.close().ok()?;
    loader.pixbuf()
}