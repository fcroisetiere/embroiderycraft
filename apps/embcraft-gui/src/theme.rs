//! The editor's look, taken from the machine it feeds: pale enamel panels like an industrial
//! embroidery machine's body, a dark control-panel rail for the tools, white stabiliser as the
//! canvas, and rayon-magenta thread as the one accent (selection, active tool, needle).

use eframe::egui::{self, Color32, CornerRadius, Stroke};

/// Rayon magenta: selection, active tool, needle position. Reads against most thread colours.
pub const ACCENT: Color32 = Color32::from_rgb(0xb0, 0x24, 0x6a);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0xf3, 0xd9, 0xe6);
/// Amber for warnings (outside hoop, over-long stitches), kept apart from the accent.
pub const WARN: Color32 = Color32::from_rgb(0xc2, 0x5a, 0x00);
/// Canvas: white tear-away stabiliser, with a faint green grid.
pub const FABRIC: Color32 = Color32::from_rgb(0xfa, 0xfa, 0xf7);
pub const GRID_MINOR: Color32 = Color32::from_rgb(0xec, 0xef, 0xea);
pub const GRID_MAJOR: Color32 = Color32::from_rgb(0xd9, 0xdf, 0xd8);
/// Wooden/plastic hoop ring.
pub const HOOP: Color32 = Color32::from_rgb(0x6f, 0x7d, 0x76);
/// Start and end of sewing: the green and red of a machine's start and stop buttons.
pub const START: Color32 = Color32::from_rgb(0x1f, 0x8a, 0x4c);
pub const END: Color32 = Color32::from_rgb(0xc0, 0x2b, 0x2b);
/// Drawing-in-progress overlay (chalk marker).
pub const DRAFT: Color32 = Color32::from_rgb(0x1f, 0x7a, 0x8c);

const ENAMEL: Color32 = Color32::from_rgb(0xe3, 0xe8, 0xe2);
const WELL: Color32 = Color32::from_rgb(0xd3, 0xdb, 0xd3);
const HOVER: Color32 = Color32::from_rgb(0xc6, 0xd0, 0xc7);
const LINE: Color32 = Color32::from_rgb(0xbf, 0xc9, 0xc0);
const INK: Color32 = Color32::from_rgb(0x1f, 0x2a, 0x27);
const MUTED: Color32 = Color32::from_rgb(0x5d, 0x6b, 0x66);

pub const RAIL: Color32 = Color32::from_rgb(0x24, 0x30, 0x2c);
const RAIL_WELL: Color32 = Color32::from_rgb(0x33, 0x42, 0x3d);
const RAIL_HOVER: Color32 = Color32::from_rgb(0x40, 0x52, 0x4c);
const RAIL_INK: Color32 = Color32::from_rgb(0xe4, 0xec, 0xe7);

fn widgets(w: &mut egui::style::Widgets, well: Color32, hover: Color32, line: Color32) {
    let r = CornerRadius::same(5);
    for s in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        s.corner_radius = r;
    }
    w.noninteractive.bg_stroke = Stroke::new(1.0, line);
    w.inactive.bg_fill = well;
    w.inactive.weak_bg_fill = well;
    w.inactive.bg_stroke = Stroke::NONE;
    w.hovered.bg_fill = hover;
    w.hovered.weak_bg_fill = hover;
    w.hovered.bg_stroke = Stroke::new(1.0, line);
    w.active.bg_fill = ACCENT;
    w.active.weak_bg_fill = ACCENT;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.open.bg_fill = hover;
    w.open.weak_bg_fill = hover;
}

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::light();
    v.panel_fill = ENAMEL;
    v.window_fill = ENAMEL;
    v.extreme_bg_color = Color32::from_rgb(0xf7, 0xf9, 0xf6);
    v.faint_bg_color = WELL;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.selection.bg_fill = ACCENT_SOFT;
    v.selection.stroke = Stroke::new(1.5, ACCENT);
    v.hyperlink_color = ACCENT;
    v.override_text_color = Some(INK);
    v.widgets.noninteractive.bg_fill = ENAMEL;
    widgets(&mut v.widgets, WELL, HOVER, LINE);
    // One look whatever the system's light/dark setting (Windows in dark mode included).
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.set_visuals_of(egui::Theme::Light, v.clone());
    ctx.set_visuals_of(egui::Theme::Dark, v);

    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.slider_width = 130.0;
    });
}

/// Visuals for the dark tool rail on the left (the machine's control panel).
pub fn rail(ui: &mut egui::Ui) {
    let v = &mut ui.style_mut().visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = RAIL;
    v.extreme_bg_color = Color32::from_rgb(0x1b, 0x25, 0x22);
    v.override_text_color = Some(RAIL_INK);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.5, Color32::WHITE);
    v.widgets.noninteractive.bg_fill = RAIL;
    widgets(&mut v.widgets, RAIL_WELL, RAIL_HOVER, RAIL_HOVER);
}

pub fn rail_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(RAIL)
        .inner_margin(egui::Margin::symmetric(8, 6))
}

/// Section heading in a side panel: sentence case, a step up in weight, never shouting.
pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(text).size(13.5).strong());
}

/// Secondary text that explains rather than labels.
pub fn hint(ui: &mut egui::Ui, text: &str) {
    let c = if ui.visuals().dark_mode {
        ui.visuals().weak_text_color()
    } else {
        MUTED
    };
    ui.label(egui::RichText::new(text).size(12.0).color(c));
}
