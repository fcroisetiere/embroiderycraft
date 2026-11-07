//! The Help menu: the online manual, bug reports and updates on embroiderycraft.org, and the
//! About window.

use crate::{App, theme};
use eframe::egui;

const SITE: &str = "https://embroiderycraft.org";
const DONATE: &str = "https://ko-fi.com/fcroisetiere";
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The system name the website's bug form understands.
fn os() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    }
}

/// A page of the manual; the website shows it in the browser's language (English or French).
pub fn manual(page: &str) -> String {
    format!("{SITE}/manual/{page}")
}

/// The website's bug form, with this version and system filled in.
fn report() -> String {
    format!("{SITE}/report.html?version={VERSION}&os={}", os())
}

fn open(ui: &egui::Ui, url: String) {
    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
}

pub fn menu(app: &mut App, ui: &mut egui::Ui) {
    ui.menu_button("Help", |ui| {
        if ui.button("User manual   F1")
            .on_hover_text("Open the EmbroideryCraft manual in your web browser")
            .clicked() {
            open(ui, manual(""));
            ui.close();
        }
        if ui.button("Keyboard shortcuts")
            .on_hover_text("Every shortcut on one page of the manual")
            .clicked() {
            open(ui, manual("shortcuts.html"));
            ui.close();
        }
        if ui.button("Problems and questions")
            .on_hover_text("Answers to common problems: fonts, trims, colours, files")
            .clicked() {
            open(ui, manual("troubleshooting.html"));
            ui.close();
        }
        ui.separator();
        if ui.button("Report a bug…")
            .on_hover_text("Tell us what went wrong. The form opens in your browser with this version filled in.")
            .clicked() {
            open(ui, report());
            ui.close();
        }
        if ui.button("Check for a new version")
            .on_hover_text("Open the download page to see the latest version")
            .clicked() {
            open(ui, format!("{SITE}/download.html"));
            ui.close();
        }
        if ui.button("Support EmbroideryCraft ♥")
            .on_hover_text("EmbroideryCraft is free. A small donation helps keep it that way.")
            .clicked() {
            open(ui, DONATE.to_string());
            ui.close();
        }
        ui.separator();
        if ui.button("About EmbroideryCraft").clicked() {
            app.about = true;
            ui.close();
        }
    });
}

pub fn about(app: &mut App, ctx: &egui::Context) {
    if !app.about {
        return;
    }
    let mut open = true;
    egui::Window::new("About EmbroideryCraft")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading(format!("EmbroideryCraft {VERSION}"));
            ui.label("A free embroidery digitizer.");
            ui.add_space(6.0);
            theme::hint(
                ui,
                "Made by Francis Croisetière, with code written together with Claude, Anthropic's AI. Free and open source under the MIT or Apache-2.0 licence.",
            );
            ui.add_space(6.0);
            ui.hyperlink_to("embroiderycraft.org", SITE);
            ui.hyperlink_to("Source code on GitHub", "https://github.com/fcroisetiere/embroiderycraft");
        });
    app.about &= open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_point_at_the_website() {
        assert_eq!(manual(""), "https://embroiderycraft.org/manual/");
        assert_eq!(
            manual("shortcuts.html"),
            "https://embroiderycraft.org/manual/shortcuts.html"
        );
        // the bug form only accepts these system names and a short version
        let r = report();
        assert!(r.starts_with("https://embroiderycraft.org/report.html?version="));
        assert!(r.ends_with("&os=Windows") || r.ends_with("&os=macOS") || r.ends_with("&os=Linux"));
        assert!(VERSION.len() <= 20);
    }
}
