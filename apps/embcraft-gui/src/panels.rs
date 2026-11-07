use crate::icons::{self, Icon};
use crate::{App, Confirm, theme};
use eframe::egui::{self, Color32, Key, Modifiers, RichText};
use embcraft_core::Point;
use embcraft_doc::{Align, Combine, EditObject, StitchStyle, Tool};
use embcraft_stitch::{FillUnderlay, SatinUnderlay, TackDown};

/// Hoop sizes (mm) offered in the View menu; any other size can be typed in.
const HOOPS: [(f64, f64); 8] = [
    (100.0, 100.0),
    (130.0, 180.0),
    (150.0, 150.0),
    (200.0, 200.0),
    (240.0, 150.0),
    (300.0, 200.0),
    (360.0, 200.0),
    (500.0, 360.0),
];

/// Zero-movement jumps per trim offered for DST export (the trim test file covers these).
const TRIM_CHOICES: [u8; 5] = [0, 2, 3, 4, 6];

fn key(ctx: &egui::Context, m: Modifiers, k: Key) -> bool {
    ctx.input_mut(|i| i.consume_key(m, k))
}

fn tool_icon(t: Tool) -> Icon {
    match t {
        Tool::Select => Icon::Select,
        Tool::Node => Icon::Node,
        Tool::Polyline => Icon::Polyline,
        Tool::Curve => Icon::Curve,
        Tool::Rectangle => Icon::Rectangle,
        Tool::Ellipse => Icon::Ellipse,
        Tool::Freehand => Icon::Freehand,
        Tool::Text => Icon::Text,
        Tool::Hole => Icon::Hole,
        Tool::Start => Icon::StartPoint,
        Tool::Direction => Icon::Direction,
    }
}

fn tool_key(t: Tool) -> &'static str {
    match t {
        Tool::Select => "V",
        Tool::Node => "N",
        Tool::Polyline => "P",
        Tool::Curve => "B",
        Tool::Rectangle => "R",
        Tool::Ellipse => "E",
        Tool::Freehand => "F",
        Tool::Text => "T",
        Tool::Hole => "H",
        Tool::Start => "S",
        Tool::Direction => "D",
    }
}

fn style_tip(icon: Icon) -> &'static str {
    match icon {
        Icon::Run => {
            "Running stitch: a single line of stitches along the shape. For outlines and fine detail."
        }
        Icon::Satin => {
            "Satin: a smooth column of zigzag stitches along the shape. For borders and lettering strokes."
        }
        Icon::AutoSatin => {
            "Auto satin: the shape is split into strokes and each is sewn as satin straight across. Best for lettering, including Kanji, Kana and Hangul."
        }
        Icon::Applique => {
            "Appliqué: a piece of fabric sewn on. The machine sews where to lay it, stops, tacks it down, stops for you to cut the spare fabric away, then covers the edge with satin."
        }
        _ => "Fill (tatami): rows of stitches covering the whole shape. For large areas.",
    }
}

/// Hover help for the property sliders, by their undo key.
fn slider_tip(key: &str) -> &'static str {
    match key {
        "run len" => "Length of each running stitch. Shorter stitches follow curves more closely.",
        "satin w" => "Width of the satin column.",
        "satin sp" => "Distance between satin stitches. Smaller is denser.",
        "satin pc" => "Widens the column to make up for the fabric pulling it narrower when sewn.",
        "fill ang" => "Direction of the fill rows, in degrees.",
        "fill sp" => "Distance between fill rows. Smaller is denser; 0.4 mm is typical.",
        "fill len" => "Length of each stitch along a row.",
        "as sp" => "Distance between satin stitches. Smaller is denser; 0.4 mm is typical.",
        "as pc" => "Widens every stroke to make up for the fabric pulling it narrower when sewn.",
        "as max" => "Strokes wider than this are sewn as split satin, so no stitch is longer.",
        "ap w" => "Width of the satin border that covers the cut edge of the fabric.",
        "ap sp" => "Distance between border stitches. Smaller is denser.",
        "ap in" => "How far inside the edge the tack-down holds the fabric.",
        "fill pc" => {
            "Lengthens rows past the outline to make up for fabric pull. Negative shortens them."
        }
        _ => "",
    }
}

fn style_icon(s: &StitchStyle) -> Icon {
    match s {
        StitchStyle::Run { .. } => Icon::Run,
        StitchStyle::Satin { .. } => Icon::Satin,
        StitchStyle::Fill { .. } => Icon::Fill,
        StitchStyle::AutoSatin { .. } => Icon::AutoSatin,
        StitchStyle::Applique { .. } => Icon::Applique,
    }
}

pub fn shortcuts(app: &mut App, ctx: &egui::Context) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let cmd = Modifiers::COMMAND;
    if key(ctx, cmd, Key::P) {
        let len = app.shown_pattern().map_or(0, |p| p.stitches.len());
        app.player.toggle(len);
    }
    if key(ctx, cmd, Key::O) {
        app.open_file();
    }
    if key(ctx, Modifiers::NONE, Key::Num0) {
        app.fit_pending = true;
    }
    if key(ctx, Modifiers::NONE, Key::F1) {
        ctx.open_url(egui::OpenUrl::new_tab(crate::help::manual("")));
    }
    if app.machine.is_some() {
        if key(ctx, Modifiers::NONE, Key::Escape) {
            app.close_machine_file();
        }
        return;
    }
    if key(ctx, cmd | Modifiers::SHIFT, Key::Z) || key(ctx, cmd, Key::Y) {
        app.ed.redo();
    } else if key(ctx, cmd, Key::Z) {
        app.ed.undo();
    }
    if key(ctx, cmd, Key::S) {
        app.save();
    }
    if key(ctx, cmd, Key::N) {
        app.request(Confirm::New);
    }
    if key(ctx, cmd, Key::E) {
        app.export_dst();
    }
    if key(ctx, cmd, Key::D) {
        app.ed.duplicate_selected(Point::new(4.0, 4.0));
    }
    if key(ctx, cmd, Key::A) {
        app.ed.select_all();
    }
    if key(ctx, cmd | Modifiers::SHIFT, Key::G) {
        app.ed.ungroup_selected();
    } else if key(ctx, cmd, Key::G) {
        app.ed.group_selected();
    }
    // While drawing, Backspace takes back the last point and never deletes the selection
    // (with the Hole tool that would be the fill being cut).
    let backspace = key(ctx, Modifiers::NONE, Key::Backspace);
    let delete = key(ctx, Modifiers::NONE, Key::Delete);
    if backspace && app.tools.undo_point() {
        // the last point was taken back
    } else if (backspace || delete) && !app.tools.is_drawing() {
        app.ed.delete_selected();
    }
    if key(ctx, Modifiers::NONE, Key::Escape) {
        app.tools.escape();
    }
    if key(ctx, Modifiers::NONE, Key::Enter) {
        app.tools.enter(&mut app.ed);
    }
    for t in Tool::ALL {
        let k = match t {
            Tool::Select => Key::V,
            Tool::Node => Key::N,
            Tool::Polyline => Key::P,
            Tool::Curve => Key::B,
            Tool::Rectangle => Key::R,
            Tool::Ellipse => Key::E,
            Tool::Freehand => Key::F,
            Tool::Text => Key::T,
            Tool::Hole => Key::H,
            Tool::Start => Key::S,
            Tool::Direction => Key::D,
        };
        if key(ctx, Modifiers::NONE, k) {
            app.tools.set_tool(t);
        }
    }
}

fn center_in_hoop(app: &mut App) {
    if app.ed.selection().is_empty() {
        app.ed.select_all();
    }
    app.ed.center_selected_at(Point::ZERO);
}

pub fn top_menu(app: &mut App, root: &mut egui::Ui) {
    egui::Panel::top("menu").show(root, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button(crate::keys("New          Ctrl+N"))
                    .on_hover_text("Start an empty design")
                    .clicked() {
                    app.request(Confirm::New);
                    ui.close();
                }
                if ui.button(crate::keys("Open…        Ctrl+O"))
                    .on_hover_text("Open a saved EmbroideryCraft project (.embcraft)")
                    .clicked() {
                    app.open_file();
                    ui.close();
                }
                if ui.button(crate::keys("Save         Ctrl+S"))
                    .on_hover_text("Save the project, to keep editing it later")
                    .clicked() {
                    app.save();
                    ui.close();
                }
                if ui.button("Save as…")
                    .on_hover_text("Save the project under a new name or in another folder")
                    .clicked() {
                    app.save_as();
                    ui.close();
                }
                ui.separator();
                if ui
                    .button("Background picture to trace…")
                    .on_hover_text("Show a logo scan or photo under the design and draw over it. It is never sewn.")
                    .clicked()
                {
                    app.pick_backdrop();
                    ui.close();
                }
                if ui
                    .button("Show on a product…")
                    .on_hover_text("See the design on a gi, belt, polo, cap… at real size, and save a mockup picture for the customer")
                    .clicked()
                {
                    app.product_picker = true;
                    ui.close();
                }
                ui.separator();
                if ui.button(crate::keys("Export DST machine file…   Ctrl+E"))
                    .on_hover_text("Write the stitches as a Tajima DST file for the embroidery machine")
                    .clicked() {
                    app.export_dst();
                    ui.close();
                }
                if ui.button("Save mockup picture…")
                    .on_hover_text("A picture (PNG) of the design on the product, with its size and thread colours, to send for approval")
                    .clicked() {
                    app.export_mockup();
                    ui.close();
                }
                if ui.button("Export SVG preview…")
                    .on_hover_text("Save a picture of the stitches (SVG) to show or print")
                    .clicked() {
                    app.export_svg();
                    ui.close();
                }
                if ui
                    .add_enabled(app.ed.selected_lettering().is_some(), egui::Button::new("Team names…"))
                    .on_hover_text("One DST file per name from a list, using the selected lettering")
                    .on_disabled_hover_text("Select a piece of lettering first")
                    .clicked()
                {
                    crate::teamnames::open(app);
                    ui.close();
                }
                ui.menu_button("Trims in DST files", |ui| {
                    ui.label("Zero-move jumps written for each trim");
                    for n in TRIM_CHOICES {
                        let text = match n {
                            0 => "None (no trims)".to_string(),
                            3 => "3 (default)".to_string(),
                            n => n.to_string(),
                        };
                        ui.radio_value(&mut app.trim_jumps, n, text);
                    }
                    theme::hint(
                        ui,
                        "Pick the smallest count that cut\nthread in your trim test.",
                    );
                });
                ui.separator();
                if ui.button("View machine file (DST)…")
                    .on_hover_text("Open a DST file to look at it and play its stitches (read only)")
                    .clicked() {
                    app.view_machine_file();
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit")
                    .on_hover_text("Close EmbroideryCraft (it asks first if there are unsaved changes)")
                    .clicked() {
                    app.request(Confirm::Quit);
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(app.ed.can_undo(), egui::Button::new(crate::keys("Undo   Ctrl+Z")))
                    .on_hover_text("Take back the last change")
                    .on_disabled_hover_text("Nothing to undo")
                    .clicked()
                {
                    app.ed.undo();
                    ui.close();
                }
                if ui
                    .add_enabled(app.ed.can_redo(), egui::Button::new(crate::keys("Redo   Ctrl+Shift+Z")))
                    .on_hover_text("Do again what was undone")
                    .on_disabled_hover_text("Nothing to redo")
                    .clicked()
                {
                    app.ed.redo();
                    ui.close();
                }
                ui.separator();
                if ui.button(crate::keys("Duplicate   Ctrl+D"))
                    .on_hover_text("Copy the selected objects, slightly offset")
                    .clicked() {
                    app.ed.duplicate_selected(Point::new(4.0, 4.0));
                    ui.close();
                }
                if ui.button("Delete   Del")
                    .on_hover_text("Remove the selected objects")
                    .clicked() {
                    app.ed.delete_selected();
                    ui.close();
                }
                if ui.button(crate::keys("Select all   Ctrl+A"))
                    .on_hover_text("Select every object in the design")
                    .clicked() {
                    app.ed.select_all();
                    ui.close();
                }
            });
            ui.menu_button("Arrange", |ui| {
                let multi = app.ed.selection().len() >= 2;
                for (label, how) in [
                    ("Align left edges", Align::Left),
                    ("Align centres horizontally", Align::CenterX),
                    ("Align right edges", Align::Right),
                    ("Align top edges", Align::Top),
                    ("Align centres vertically", Align::CenterY),
                    ("Align bottom edges", Align::Bottom),
                ] {
                    if ui
                        .add_enabled(multi, egui::Button::new(label))
                        .on_hover_text("Line the selected objects up with each other")
                        .on_disabled_hover_text("Select two or more objects to align them")
                        .clicked()
                    {
                        app.ed.align_selected(how);
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Centre in hoop")
                    .on_hover_text("Move the selection (or the whole design) to the middle of the hoop")
                    .clicked() {
                    center_in_hoop(app);
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(multi, egui::Button::new(crate::keys("Group   Ctrl+G")))
                    .on_hover_text("Keep the selected objects together: a click selects them all")
                    .on_disabled_hover_text("Select two or more objects to group them")
                    .clicked()
                {
                    app.ed.group_selected();
                    ui.close();
                }
                let grouped = app.ed.selected_objects().any(|o| o.group.is_some());
                if ui
                    .add_enabled(grouped, egui::Button::new(crate::keys("Ungroup   Ctrl+Shift+G")))
                    .on_hover_text("Take the selected objects out of their group")
                    .on_disabled_hover_text("The selection is not grouped")
                    .clicked()
                {
                    app.ed.ungroup_selected();
                    ui.close();
                }
                ui.separator();
                let can = combinable(app) >= 2;
                for op in Combine::ALL {
                    if ui
                        .add_enabled(can, egui::Button::new(format!("Shapes: {}", op.label())))
                        .on_hover_text(op.description())
                        .on_disabled_hover_text("Select two or more closed shapes")
                        .clicked()
                    {
                        combine(app, op);
                        ui.close();
                    }
                }
            });
            ui.menu_button("Fonts", |ui| {
                if ui
                    .button("Make an EmbroideryCraft font…")
                    .on_hover_text("Convert a computer font (Japanese, Korean, Latin…) into satin lettering, once")
                    .clicked()
                {
                    app.font_maker = Some(crate::fonts::FontMaker::default());
                    ui.close();
                }
                ui.menu_button("Edit an EmbroideryCraft font", |ui| {
                    if app.ec_fonts.is_empty() {
                        theme::hint(ui, "No EmbroideryCraft fonts yet.");
                    }
                    for (name, path) in app.ec_fonts.clone() {
                        let src = crate::font_preview::Source::Ec(path.clone());
                        if app.previews.row(ui, &src, &name, false).clicked() {
                            match embcraft_doc::font::EcFont::load(&path) {
                                Ok(f) => crate::fonts::begin_edit(app, f, path),
                                Err(e) => app.status = e.to_string(),
                            }
                            ui.close();
                        }
                    }
                });
                if ui
                    .button("New blank font…")
                    .on_hover_text("Start an empty EmbroideryCraft font and draw its characters yourself")
                    .clicked()
                {
                    let mut n = 1;
                    let f = loop {
                        let f = embcraft_doc::font::EcFont::blank(&format!("My font {n}"));
                        if !f.default_path().exists() {
                            break f;
                        }
                        n += 1;
                    };
                    let path = f.default_path();
                    let _ = f.save(&path);
                    crate::fonts::begin_edit(app, f, path);
                    ui.close();
                }
                ui.separator();
                theme::hint(
                    ui,
                    &format!("Fonts are kept in\n{}", embcraft_doc::font::fonts_dir().display()),
                );
            });
            ui.menu_button("View", |ui| {
                ui.checkbox(&mut app.show_stitches, "Show stitches")
                    .on_hover_text("Draw the stitches; off shows only the outlines of the shapes");
                ui.checkbox(&mut app.show_grid, "Show grid")
                    .on_hover_text("A millimetre grid behind the design");
                ui.checkbox(&mut app.show_hoop, "Show hoop")
                    .on_hover_text("The hoop outline, to check the design fits");
                ui.checkbox(&mut app.realistic, "Realistic stitches")
                    .on_hover_text("Draw stitches as shaded thread, as they look sewn");
                if ui.button("Show on a product…")
                    .on_hover_text("See the design on a gi, belt, polo, cap… at real size")
                    .clicked() {
                    app.product_picker = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Zoom to fit   0")
                    .on_hover_text("Show the whole hoop and design")
                    .clicked() {
                    app.fit_pending = true;
                    ui.close();
                }
                if ui.button(crate::keys("Stitch player   Ctrl+P"))
                    .on_hover_text("Watch the design sew out stitch by stitch")
                    .clicked() {
                    let len = app.shown_pattern().map_or(0, |p| p.stitches.len());
                    app.player.toggle(len);
                    ui.close();
                }
                ui.separator();
                ui.menu_button("Hoop size", |ui| {
                    for (w, h) in HOOPS {
                        let on = app.hoop == (w, h);
                        if ui.radio(on, format!("{w:.0} × {h:.0} mm")).clicked() {
                            app.hoop = (w, h);
                            ui.close();
                        }
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Custom");
                        ui.add(
                            egui::DragValue::new(&mut app.hoop.0)
                                .range(20.0..=1000.0)
                                .speed(1.0),
                        );
                        ui.label("×");
                        ui.add(
                            egui::DragValue::new(&mut app.hoop.1)
                                .range(20.0..=1000.0)
                                .speed(1.0),
                        );
                        ui.label("mm");
                    });
                });
            });
            crate::help::menu(app, ui);
        });
    });
}

fn group(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
}

pub fn toolbar(app: &mut App, root: &mut egui::Ui) {
    egui::Panel::top("toolbar")
        .frame(
            egui::Frame::side_top_panel(root.style()).inner_margin(egui::Margin::symmetric(8, 4)),
        )
        .show(root, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let editing = app.machine.is_none();
                let has_sel = editing && !app.ed.selection().is_empty();
                let multi = editing && app.ed.selection().len() >= 2;

                if icons::button(
                    ui,
                    Icon::New,
                    false,
                    editing,
                    &crate::keys("New design (Ctrl+N)"),
                )
                .clicked()
                {
                    app.request(Confirm::New);
                }
                if icons::button(
                    ui,
                    Icon::Open,
                    false,
                    true,
                    &crate::keys("Open a project (Ctrl+O)"),
                )
                .clicked()
                {
                    app.open_file();
                }
                if icons::button(
                    ui,
                    Icon::Save,
                    false,
                    editing,
                    &crate::keys("Save the project (Ctrl+S)"),
                )
                .clicked()
                {
                    app.save();
                }
                if icons::button(
                    ui,
                    Icon::Export,
                    false,
                    editing,
                    &crate::keys("Export a DST file for the machine (Ctrl+E)"),
                )
                .clicked()
                {
                    app.export_dst();
                }
                if icons::button(
                    ui,
                    Icon::MachineFile,
                    app.machine.is_some(),
                    true,
                    "View a DST machine file",
                )
                .clicked()
                {
                    if app.machine.is_some() {
                        app.close_machine_file();
                    } else {
                        app.view_machine_file();
                    }
                }
                group(ui);
                if icons::button(
                    ui,
                    Icon::Undo,
                    false,
                    editing && app.ed.can_undo(),
                    &crate::keys("Undo (Ctrl+Z)"),
                )
                .clicked()
                {
                    app.ed.undo();
                }
                if icons::button(
                    ui,
                    Icon::Redo,
                    false,
                    editing && app.ed.can_redo(),
                    &crate::keys("Redo (Ctrl+Shift+Z)"),
                )
                .clicked()
                {
                    app.ed.redo();
                }
                group(ui);
                if icons::button(
                    ui,
                    Icon::Duplicate,
                    false,
                    has_sel,
                    &crate::keys("Duplicate (Ctrl+D)"),
                )
                .clicked()
                {
                    app.ed.duplicate_selected(Point::new(4.0, 4.0));
                }
                if icons::button(ui, Icon::Delete, false, has_sel, "Delete (Del)").clicked() {
                    app.ed.delete_selected();
                }
                group(ui);
                arrange_buttons(app, ui, has_sel);
                group(ui);
                for (icon, how, tip) in [
                    (Icon::AlignLeft, Align::Left, "Align left edges"),
                    (
                        Icon::AlignCenterX,
                        Align::CenterX,
                        "Align centres horizontally",
                    ),
                    (Icon::AlignRight, Align::Right, "Align right edges"),
                    (Icon::AlignTop, Align::Top, "Align top edges"),
                    (
                        Icon::AlignCenterY,
                        Align::CenterY,
                        "Align centres vertically",
                    ),
                    (Icon::AlignBottom, Align::Bottom, "Align bottom edges"),
                ] {
                    let r = icons::button(ui, icon, false, multi, tip);
                    if r.clicked() {
                        app.ed.align_selected(how);
                    }
                }
                if icons::button(
                    ui,
                    Icon::CenterHoop,
                    false,
                    editing && !app.ed.doc().objects.is_empty(),
                    "Centre in the hoop (the whole design if nothing is selected)",
                )
                .clicked()
                {
                    center_in_hoop(app);
                }
                group(ui);
                ui.add_enabled_ui(editing, |ui| shape_buttons(app, ui));
                group(ui);
                if icons::button(
                    ui,
                    Icon::Stitches,
                    app.show_stitches,
                    editing,
                    "Show stitches",
                )
                .clicked()
                {
                    app.show_stitches = !app.show_stitches;
                }
                if icons::button(ui, Icon::Grid, app.show_grid, true, "Show grid").clicked() {
                    app.show_grid = !app.show_grid;
                }
                if icons::button(
                    ui,
                    Icon::Hoop,
                    app.show_hoop,
                    true,
                    &format!(
                        "Show the {:.0} × {:.0} mm hoop (change it in View)",
                        app.hoop.0, app.hoop.1
                    ),
                )
                .clicked()
                {
                    app.show_hoop = !app.show_hoop;
                }
                if icons::button(ui, Icon::ZoomFit, false, true, "Zoom to fit (0)").clicked() {
                    app.fit_pending = true;
                }
                if icons::button(
                    ui,
                    Icon::Player,
                    app.player.open,
                    true,
                    &crate::keys("Stitch player: watch the design sew out (Ctrl+P)"),
                )
                .clicked()
                {
                    let len = app.shown_pattern().map_or(0, |p| p.stitches.len());
                    app.player.toggle(len);
                }
            });
        });
    if let Some(m) = &app.machine {
        let name = m
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut close = false;
        egui::Panel::top("machine_banner")
            .frame(
                egui::Frame::new()
                    .fill(theme::ACCENT_SOFT)
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Viewing {name}")).strong());
                    ui.label("Read-only. Orange crosses mark trims; use the stitch player to watch it sew.");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close = ui.button("Back to the design   Esc").clicked();
                    });
                });
            });
        if close {
            app.close_machine_file();
        }
    }
}

/// Rotate, flip, resize and sewing order: shared by the toolbar and the properties panel.
/// Seven spots of a box (corners, top and bottom centre, middle) as a small grid of buttons.
/// `current` marks the one in use; returns the one clicked, as fractions across and down.
fn anchor_grid(ui: &mut egui::Ui, current: Option<(f64, f64)>, of: &str) -> Option<(f64, f64)> {
    let mut picked = None;
    let cell = 20.0;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(cell * 3.0 + 6.0, cell * 3.0 + 6.0),
        egui::Sense::hover(),
    );
    let p = ui.painter();
    p.rect_stroke(
        rect.shrink(cell / 2.0 + 3.0),
        0.0,
        egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
        egui::StrokeKind::Middle,
    );
    for (fx, fy) in [
        (0.0, 0.0),
        (0.5, 0.0),
        (1.0, 0.0),
        (0.5, 0.5),
        (0.0, 1.0),
        (0.5, 1.0),
        (1.0, 1.0),
    ] {
        let c = rect.min
            + egui::vec2(3.0 + cell / 2.0, 3.0 + cell / 2.0)
            + egui::vec2(fx as f32 * cell * 2.0, fy as f32 * cell * 2.0);
        let r = egui::Rect::from_center_size(c, egui::vec2(cell, cell));
        let resp = ui.interact(
            r,
            ui.id()
                .with(("anchor", of, (fx * 2.0) as u8, (fy * 2.0) as u8)),
            egui::Sense::click(),
        );
        let on = current.is_some_and(|(x, y)| (x - fx).abs() < 0.02 && (y - fy).abs() < 0.02);
        let (fill, rad) = if on {
            (theme::START, 6.0)
        } else if resp.hovered() {
            (theme::ACCENT, 5.0)
        } else {
            (ui.visuals().text_color(), 3.5)
        };
        ui.painter()
            .circle(c, rad, fill, egui::Stroke::new(1.5, Color32::WHITE));
        let name = match ((fx * 2.0) as u8, (fy * 2.0) as u8) {
            (1, 1) => "the middle".to_string(),
            (1, 0) => "the top centre".into(),
            (1, _) => "the bottom centre".into(),
            (0, 0) => "the top left corner".into(),
            (_, 0) => "the top right corner".into(),
            (0, _) => "the bottom left corner".into(),
            _ => "the bottom right corner".into(),
        };
        if resp
            .on_hover_text(format!("Start at {name} {of}"))
            .clicked()
        {
            picked = Some((fx, fy));
        }
    }
    picked
}

/// Where the machine starts and ends the whole design (shown when nothing is selected).
fn design_start(app: &mut App, ui: &mut egui::Ui) {
    use embcraft_design::DesignStart;
    theme::section(ui, "Machine start and end");
    let cur = app.ed.doc().design_start;
    theme::hint(
        ui,
        if cur == DesignStart::HoopCentre {
            "The machine starts at the hoop centre and the design keeps its place in the hoop."
        } else {
            "Put the needle on this point of the garment before pressing start (the green cross). The design ends back there."
        },
    );
    ui.horizontal(|ui| {
        if let Some((fx, fy)) = anchor_grid(ui, cur.anchor(), "of the design") {
            if let Some(at) = DesignStart::ALL
                .into_iter()
                .find(|d| d.anchor() == Some((fx, fy)))
            {
                app.ed.set_design_start(at);
            }
        }
        ui.vertical(|ui| {
            ui.label(cur.label());
            if ui
                .add_enabled(
                    cur != DesignStart::HoopCentre,
                    egui::Button::new("Hoop centre"),
                )
                .on_hover_text("Start at the hoop centre, with the design where it is in the hoop")
                .clicked()
            {
                app.ed.set_design_start(DesignStart::HoopCentre);
            }
        });
    });
}

/// How many selected objects can be combined as shapes.
fn combinable(app: &App) -> usize {
    app.ed
        .selected_objects()
        .filter(|o| embcraft_doc::can_combine(o))
        .count()
}

fn combine(app: &mut App, op: Combine) {
    app.status = match app.ed.combine_selected(op) {
        Ok(1) => format!("{}: one shape", op.label()),
        Ok(n) => format!("{}: {n} shapes", op.label()),
        Err(e) => e,
    };
}

fn shape_icon(op: Combine) -> Icon {
    match op {
        Combine::Add => Icon::ShapeAdd,
        Combine::Subtract => Icon::ShapeSubtract,
        Combine::Overlap => Icon::ShapeOverlap,
        Combine::Exclude => Icon::ShapeExclude,
    }
}

/// Group, ungroup and the four ways of combining shapes, as icon buttons.
fn shape_buttons(app: &mut App, ui: &mut egui::Ui) {
    let multi = app.ed.selection().len() >= 2;
    let grouped = app.ed.selected_objects().any(|o| o.group.is_some());
    if icons::button(
        ui,
        Icon::Group,
        false,
        multi && !app.ed.selection_is_group(),
        &crate::keys("Group: keep the selected objects together (Ctrl+G)"),
    )
    .clicked()
    {
        app.ed.group_selected();
    }
    if icons::button(
        ui,
        Icon::Ungroup,
        false,
        grouped,
        &crate::keys("Ungroup (Ctrl+Shift+G)"),
    )
    .clicked()
    {
        app.ed.ungroup_selected();
    }
    let can = combinable(app) >= 2;
    for op in Combine::ALL {
        let tip = format!("{}: {}", op.label(), op.description().to_lowercase());
        if icons::button(ui, shape_icon(op), false, can, &tip).clicked() {
            combine(app, op);
        }
    }
}

fn arrange_buttons(app: &mut App, ui: &mut egui::Ui, enabled: bool) {
    let acts: [(Icon, &str); 8] = [
        (Icon::RotateLeft, "Rotate 15° left"),
        (Icon::RotateRight, "Rotate 15° right"),
        (Icon::FlipH, "Flip horizontally"),
        (Icon::FlipV, "Flip vertically"),
        (Icon::Grow, "Enlarge by 10 %"),
        (Icon::Shrink, "Reduce by 10 %"),
        (Icon::SewEarlier, "Sew earlier"),
        (Icon::SewLater, "Sew later"),
    ];
    for (icon, tip) in acts {
        if icons::button(ui, icon, false, enabled, tip).clicked() {
            match icon {
                Icon::RotateLeft => app.ed.rotate_selected(-15.0),
                Icon::RotateRight => app.ed.rotate_selected(15.0),
                Icon::FlipH => app.ed.flip_selected(true),
                Icon::FlipV => app.ed.flip_selected(false),
                Icon::Grow => app.ed.scale_selected(1.1),
                Icon::Shrink => app.ed.scale_selected(1.0 / 1.1),
                Icon::SewEarlier => app.ed.move_selected_order(false),
                _ => app.ed.move_selected_order(true),
            }
        }
    }
}

pub fn left_panel(app: &mut App, root: &mut egui::Ui) {
    egui::Panel::left("tools")
        .resizable(false)
        .exact_size(176.0)
        .frame(theme::rail_frame())
        .show(root, |ui| {
            theme::rail(ui);
            egui::ScrollArea::vertical().show(ui, |ui| rail_contents(app, ui));
        });
}

fn rail_contents(app: &mut App, ui: &mut egui::Ui) {
    if app.machine.is_some() {
        ui.disable();
    }
    let w = ui.available_width();
    theme::section(ui, "Tools");
    for t in Tool::ALL {
        let sel = app.tools.tool == t;
        let r = icons::labeled(ui, tool_icon(t), t.label(), tool_key(t), sel, w)
            .on_hover_text(format!("{}  (key {})", t.description(), tool_key(t)));
        if r.clicked() && !sel {
            app.tools.set_tool(t);
        }
    }
    theme::section(ui, "New shapes stitch as");
    let width = match &app.tools.defaults.style {
        StitchStyle::Satin { width, .. } => *width,
        _ => 3.0,
    };
    let cur = &app.tools.defaults.style;
    let picks = [
        (Icon::Run, "Run", matches!(cur, StitchStyle::Run { .. })),
        (
            Icon::Satin,
            "Satin",
            matches!(cur, StitchStyle::Satin { .. }),
        ),
        (Icon::Fill, "Fill", cur.is_fill()),
        (
            Icon::AutoSatin,
            "Auto satin",
            matches!(cur, StitchStyle::AutoSatin { .. }),
        ),
    ];
    for (icon, label, on) in picks {
        if icons::labeled(ui, icon, label, "", on, w)
            .on_hover_text(style_tip(icon))
            .clicked()
        {
            app.tools.defaults.style = match icon {
                Icon::Run => StitchStyle::run(),
                Icon::Satin => StitchStyle::satin(width),
                Icon::AutoSatin => StitchStyle::auto_satin(),
                _ => StitchStyle::fill(),
            };
        }
    }
    if let StitchStyle::Satin { width, .. } = &mut app.tools.defaults.style {
        ui.spacing_mut().slider_width = w - 60.0;
        ui.add(egui::Slider::new(width, 0.8..=12.0).suffix(" mm"));
    }
    theme::section(ui, "Thread");
    thread_palette(ui, app, None);
    theme::section(ui, "Moving around");
    theme::hint(
        ui,
        &format!(
            "{}\n\nPolyline: click points; finish on the first point, with a double-click or with Enter. Backspace takes back a point.\n\nHole: click a fill, then click around the area to leave empty. You never need to hit the first point: Enter or a double-click closes the shape.",
            if cfg!(target_os = "macos") {
                "Pinch to zoom, two fingers to move around (or ⌘ + scroll to zoom). Hold Space and drag, or drag with the right button, to pan."
            } else {
                "Scroll to zoom. Drag with the middle or right button, or hold Space, to pan."
            }
        ),
    );
}

/// Palette buttons. With `Some(())` clicking applies the thread to the selection.
fn thread_palette(ui: &mut egui::Ui, app: &mut App, apply_to_selection: Option<()>) {
    let threads: Vec<([u8; 3], String)> = app
        .ed
        .doc()
        .threads
        .iter()
        .map(|t| (t.rgb, t.name.clone()))
        .collect();
    let current = if apply_to_selection.is_some() {
        app.selected_first().map(|o| o.thread)
    } else {
        Some(app.tools.defaults.thread)
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for (i, (rgb, name)) in threads.iter().enumerate() {
            let on = current == Some(i);
            let b = egui::Button::new("")
                .fill(Color32::from_rgb(rgb[0], rgb[1], rgb[2]))
                .min_size(egui::vec2(22.0, 22.0))
                .corner_radius(11)
                .stroke(if on {
                    egui::Stroke::new(2.5, theme::ACCENT)
                } else {
                    egui::Stroke::new(1.0, Color32::from_gray(140))
                });
            if ui
                .add(b)
                .on_hover_text(format!("{} (thread {})", name, i + 1))
                .clicked()
            {
                if apply_to_selection.is_some() {
                    app.ed.set_thread_selected(i);
                } else {
                    app.tools.defaults.thread = i;
                }
            }
        }
        if ui
            .add(
                egui::Button::new("+")
                    .min_size(egui::vec2(22.0, 22.0))
                    .corner_radius(11),
            )
            .on_hover_text("Add a thread colour")
            .clicked()
        {
            app.ed.checkpoint();
            let n = app.ed.doc().threads.len() + 1;
            app.ed.doc_mut().threads.push(embcraft_core::Thread::new(
                [0x90, 0x90, 0x90],
                &format!("Thread {n}"),
            ));
        }
    });
    let idx = current.unwrap_or(0);
    if let Some(t) = app.ed.doc().threads.get(idx) {
        let mut rgb = t.rgb;
        let name = t.name.clone();
        ui.horizontal(|ui| {
            if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
                app.ed.checkpoint_keyed("thread colour");
                if let Some(t) = app.ed.doc_mut().threads.get_mut(idx) {
                    t.rgb = rgb;
                }
            }
            theme::hint(ui, &format!("{name}, thread {}", idx + 1));
        });
    }
}

macro_rules! slider {
    ($ui:expr, $app:expr, $key:expr, $label:expr, $val:expr, $range:expr, $apply:expr) => {{
        let mut v: f64 = $val;
        if $ui
            .add(egui::Slider::new(&mut v, $range).text($label))
            .on_hover_text(slider_tip($key))
            .changed()
        {
            $app.ed
                .edit_selected_keyed($key, |o: &mut EditObject| ($apply)(o, v));
        }
    }};
}

/// A small icon drawn inline (object list rows).
fn inline_icon(ui: &mut egui::Ui, icon: Icon) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    icons::paint(ui.painter(), r, icon, ui.visuals().text_color());
}

pub fn right_panel(app: &mut App, root: &mut egui::Ui) {
    egui::Panel::right("props")
        .resizable(true)
        .default_size(300.0)
        .show(root, |ui| {
            if app.text.is_some() && app.text_docked {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut app.lettering_tab, false, "Properties");
                    ui.selectable_value(&mut app.lettering_tab, true, "Lettering")
                        .on_hover_text("The lettering being set up");
                });
                ui.separator();
                if app.lettering_tab {
                    crate::text::panel(app, ui);
                    return;
                }
            }
            egui::ScrollArea::vertical()
                .id_salt("props_scroll")
                .show(ui, |ui| right_contents(app, ui));
        });
}

fn right_contents(app: &mut App, ui: &mut egui::Ui) {
    {
        {
            if let Some(m) = &app.machine {
                machine_info(ui, &m.pattern);
                return;
            }
            crate::fonts::edit_panel(app, ui);
            crate::backdrop::panel(app, ui);
            crate::product::panel(app, ui);
            theme::section(ui, "Sewing order");
            theme::hint(ui, "Top of the list sews last.");
            let rows: Vec<(u64, String, bool, Icon)> = app
                .ed
                .doc()
                .objects
                .iter()
                .rev()
                .map(|o| (o.id, o.name.clone(), o.visible, style_icon(&o.style)))
                .collect();
            egui::ScrollArea::vertical()
                .id_salt("objs")
                .max_height(170.0)
                .show(ui, |ui| {
                    if rows.is_empty() {
                        theme::hint(ui, "Shapes you draw appear here.");
                    }
                    for (id, name, vis, icon) in rows {
                        ui.horizontal(|ui| {
                            let mut v = vis;
                            if ui
                                .checkbox(&mut v, "")
                                .on_hover_text("Stitch this object")
                                .changed()
                            {
                                app.ed.checkpoint();
                                if let Some(o) = app.ed.doc_mut().object_mut(id) {
                                    o.visible = v;
                                }
                            }
                            inline_icon(ui, icon);
                            let sel = app.ed.is_selected(id);
                            if ui.selectable_label(sel, name).clicked() {
                                if ui.input(|i| i.modifiers.shift) {
                                    app.ed.toggle_select(id);
                                } else {
                                    app.ed.select_only(id);
                                }
                            }
                        });
                    }
                });
            ui.separator();
            theme::section(ui, "Properties");
            let Some(first) = app.selected_first().cloned() else {
                theme::hint(ui, "Select an object to change how it is stitched.");
                design_start(app, ui);
                return;
            };
            if let Some(block) = app.ed.selected_lettering() {
                let title = app
                    .ed
                    .doc()
                    .letterings
                    .get(&block)
                    .map(|b| b.spec.title())
                    .unwrap_or_default();
                theme::hint(ui, &format!("Lettering \"{title}\""));
                ui.horizontal(|ui| {
                    if ui
                        .button("Edit text…")
                        .on_hover_text("Retype the text, or change its font, size or arc. It stays where you put it.")
                        .clicked()
                    {
                        crate::text::edit_selected(app);
                    }
                    if ui
                        .button("Team names…")
                        .on_hover_text("One DST file per name from a list")
                        .clicked()
                    {
                        crate::teamnames::open(app);
                    }
                });
                theme::hint(
                    ui,
                    "Double-click a letter to move, resize or restitch it on its own.",
                );
            } else if first.lettering.is_some() {
                theme::hint(
                    ui,
                    "One letter of a lettering, selected on its own. Click the lettering once to select all of it again and retype it.",
                );
            }
            let n = app.ed.selection().len();
            if n > 1 {
                theme::hint(
                    ui,
                    &format!(
                        "{n} objects selected. Changes apply to every one of the same stitch type."
                    ),
                );
            } else {
                let mut name = first.name.clone();
                if ui.text_edit_singleline(&mut name).changed() {
                    app.ed
                        .edit_selected_keyed("name", |o| o.name = name.clone());
                }
            }

            // stitch type as four icon buttons, two per row
            ui.horizontal_wrapped(|ui| {
                let width = match &first.style {
                    StitchStyle::Satin { width, .. } => *width,
                    _ => 3.0,
                };
                let w = (ui.available_width() - 8.0) / 2.0;
                for (icon, label) in [
                    (Icon::Run, "Run"),
                    (Icon::Satin, "Satin"),
                    (Icon::Fill, "Fill"),
                    (Icon::AutoSatin, "Auto satin"),
                    (Icon::Applique, "Appliqué"),
                ] {
                    let on = style_icon(&first.style) == icon;
                    if icons::labeled(ui, icon, label, "", on, w)
                        .on_hover_text(style_tip(icon))
                        .clicked()
                        && !on
                    {
                        app.ed.checkpoint();
                        let sel = app.ed.selection().to_vec();
                        for o in app
                            .ed
                            .doc_mut()
                            .objects
                            .iter_mut()
                            .filter(|o| sel.contains(&o.id))
                        {
                            o.style = match icon {
                                Icon::Run => StitchStyle::run(),
                                Icon::Satin => StitchStyle::satin(width),
                                Icon::AutoSatin => {
                                    o.closed = true;
                                    StitchStyle::auto_satin()
                                }
                                Icon::Applique => {
                                    o.closed = true;
                                    StitchStyle::applique()
                                }
                                _ => {
                                    o.closed = true;
                                    StitchStyle::fill()
                                }
                            };
                        }
                    }
                }
            });

            match &first.style {
                StitchStyle::Run { params } => {
                    slider!(
                        ui,
                        app,
                        "run len",
                        "Stitch length mm",
                        params.stitch_len,
                        0.8..=7.0,
                        |o: &mut EditObject, v| if let StitchStyle::Run { params } = &mut o.style {
                            params.stitch_len = v
                        }
                    );
                    let mut triple = params.triple;
                    if ui
                .checkbox(&mut triple, "Triple run")
                .on_hover_text("Sew each stitch three times (forward, back, forward) for a bolder line")
                .changed()
            {
                        app.ed.edit_selected_keyed("triple", |o| {
                            if let StitchStyle::Run { params } = &mut o.style {
                                params.triple = triple
                            }
                        });
                    }
                }
                StitchStyle::Satin { width, params } => {
                    slider!(
                        ui,
                        app,
                        "satin w",
                        "Width mm",
                        *width,
                        0.8..=12.0,
                        |o: &mut EditObject, v| if let StitchStyle::Satin { width, .. } =
                            &mut o.style
                        {
                            *width = v
                        }
                    );
                    slider!(
                        ui,
                        app,
                        "satin sp",
                        "Spacing mm",
                        params.spacing,
                        0.2..=1.0,
                        |o: &mut EditObject, v| if let StitchStyle::Satin { params, .. } =
                            &mut o.style
                        {
                            params.spacing = v
                        }
                    );
                    slider!(
                        ui,
                        app,
                        "satin pc",
                        "Pull comp mm",
                        params.pull_comp,
                        0.0..=1.0,
                        |o: &mut EditObject, v| if let StitchStyle::Satin { params, .. } =
                            &mut o.style
                        {
                            params.pull_comp = v
                        }
                    );
                    let mut u = params.underlay;
                    let name = |c: SatinUnderlay| match c {
                        SatinUnderlay::None => "None",
                        SatinUnderlay::Center => "Centre run",
                        SatinUnderlay::Edge => "Edge run",
                        SatinUnderlay::CenterEdge => "Centre and edge",
                    };
                    egui::ComboBox::from_label("Underlay")
                        .selected_text(name(u))
                        .show_ui(ui, |ui| {
                            for c in [
                                SatinUnderlay::None,
                                SatinUnderlay::Center,
                                SatinUnderlay::Edge,
                                SatinUnderlay::CenterEdge,
                            ] {
                                ui.selectable_value(&mut u, c, name(c));
                            }
                        })
                        .response
                        .on_hover_text("Stitches sewn first under the satin to hold the fabric and lift the column");
                    if u != params.underlay {
                        app.ed.edit_selected_keyed("satin ul", |o| {
                            if let StitchStyle::Satin { params, .. } = &mut o.style {
                                params.underlay = u
                            }
                        });
                    }
                }
                StitchStyle::AutoSatin { params, .. } => {
                    slider!(
                        ui,
                        app,
                        "as sp",
                        "Spacing mm",
                        params.spacing,
                        0.25..=0.8,
                        |o: &mut EditObject, v| if let StitchStyle::AutoSatin { params, .. } =
                            &mut o.style
                        {
                            params.spacing = v
                        }
                    );
                    slider!(
                        ui,
                        app,
                        "as pc",
                        "Pull comp mm",
                        params.pull_comp,
                        0.0..=0.6,
                        |o: &mut EditObject, v| if let StitchStyle::AutoSatin { params, .. } =
                            &mut o.style
                        {
                            params.pull_comp = v
                        }
                    );
                    slider!(
                        ui,
                        app,
                        "as max",
                        "Longest stitch mm",
                        params.max_stitch,
                        3.0..=12.0,
                        |o: &mut EditObject, v| if let StitchStyle::AutoSatin { params, .. } =
                            &mut o.style
                        {
                            params.max_stitch = v
                        }
                    );
                    let mut ul = params.underlay;
                    if ui
                        .checkbox(&mut ul, "Underlay")
                        .on_hover_text("A run along each stroke, sewn first to hold the fabric and lift the satin")
                        .changed()
                    {
                        app.ed.edit_selected_keyed("as ul", |o| {
                            if let StitchStyle::AutoSatin { params, .. } = &mut o.style {
                                params.underlay = ul
                            }
                        });
                    }
                    if let Some(r) = first.satin_regions() {
                        ui.horizontal(|ui| {
                            theme::hint(ui, &format!("{} satin directions set", r.len()));
                            if ui
                                .small_button("Work out again")
                                .on_hover_text("Forget the stored stitch directions and work them out from the shape")
                                .clicked()
                            {
                                app.ed.edit_selected_keyed("as reset", |o| o.set_satin_regions(Vec::new()));
                            }
                        });
                    }
                }
                StitchStyle::Applique { params } => {
                    let mut p = params.clone();
                    ui.checkbox(&mut p.placement, "Placement line, then stop")
                        .on_hover_text("Sew the outline and stop, so you can see where to lay the fabric. Turn off for fabric cut to shape beforehand.");
                    ui.horizontal(|ui| {
                        ui.label("Tack-down");
                        ui.radio_value(&mut p.tack, TackDown::Run, "Run")
                            .on_hover_text("A running stitch just inside the edge");
                        ui.radio_value(&mut p.tack, TackDown::Zigzag, "Zigzag")
                            .on_hover_text("An open zigzag over the edge: holds thick or slippery fabric better");
                    });
                    ui.checkbox(&mut p.trim_stop, "Stop to cut the fabric")
                        .on_hover_text("Stop after the tack-down so the spare fabric can be cut away before the border");
                    if p != *params {
                        app.ed.edit_selected_keyed("ap opts", |o| {
                            if let StitchStyle::Applique { params } = &mut o.style {
                                params.placement = p.placement;
                                params.tack = p.tack;
                                params.trim_stop = p.trim_stop;
                            }
                        });
                    }
                    slider!(
                        ui,
                        app,
                        "ap w",
                        "Border width mm",
                        params.cover_width,
                        1.0..=8.0,
                        |o: &mut EditObject, v| if let StitchStyle::Applique { params } =
                            &mut o.style
                        {
                            params.cover_width = v
                        }
                    );
                    slider!(
                        ui,
                        app,
                        "ap sp",
                        "Border spacing mm",
                        params.cover_spacing,
                        0.25..=1.0,
                        |o: &mut EditObject, v| if let StitchStyle::Applique { params } =
                            &mut o.style
                        {
                            params.cover_spacing = v
                        }
                    );
                    slider!(
                        ui,
                        app,
                        "ap in",
                        "Tack-down inset mm",
                        params.tack_inset,
                        0.0..=2.0,
                        |o: &mut EditObject, v| if let StitchStyle::Applique { params } =
                            &mut o.style
                        {
                            params.tack_inset = v
                        }
                    );
                    let stops = 1 + usize::from(p.placement) + usize::from(p.trim_stop);
                    theme::hint(
                        ui,
                        &format!(
                            "Sews in {stops} steps; the machine stops between them (a colour change in the same thread)."
                        ),
                    );
                }
                StitchStyle::Fill { params } => {
                    slider!(
                        ui,
                        app,
                        "fill ang",
                        "Angle °",
                        params.angle_deg,
                        0.0..=180.0,
                        |o: &mut EditObject, v| if let StitchStyle::Fill { params } = &mut o.style {
                            params.angle_deg = v
                        }
                    );
                    slider!(
                        ui,
                        app,
                        "fill sp",
                        "Row spacing mm",
                        params.row_spacing,
                        0.2..=1.0,
                        |o: &mut EditObject, v| if let StitchStyle::Fill { params } = &mut o.style {
                            params.row_spacing = v
                        }
                    );
                    slider!(
                        ui,
                        app,
                        "fill len",
                        "Stitch length mm",
                        params.stitch_len,
                        1.0..=7.0,
                        |o: &mut EditObject, v| if let StitchStyle::Fill { params } = &mut o.style {
                            params.stitch_len = v
                        }
                    );
                    slider!(
                        ui,
                        app,
                        "fill pc",
                        "Pull comp mm",
                        params.pull_comp,
                        -0.5..=1.0,
                        |o: &mut EditObject, v| if let StitchStyle::Fill { params } = &mut o.style {
                            params.pull_comp = v
                        }
                    );
                    let mut ul = params.underlay.is_some();
                    if ui
                        .checkbox(&mut ul, "Underlay")
                        .on_hover_text(
                            "A light layer of stitches sewn first to hold the fabric flat",
                        )
                        .changed()
                    {
                        app.ed.edit_selected_keyed("fill ul", |o| {
                            if let StitchStyle::Fill { params } = &mut o.style {
                                params.underlay = if ul {
                                    Some(FillUnderlay::default())
                                } else {
                                    None
                                }
                            }
                        });
                    }
                    if !first.holes.is_empty() {
                        ui.horizontal(|ui| {
                            theme::hint(ui, &format!("{} hole(s)", first.holes.len()));
                            if ui
                                .small_button("Remove holes")
                                .on_hover_text("Fill the holes in again")
                                .clicked()
                            {
                                app.ed.checkpoint();
                                let sel = app.ed.selection().to_vec();
                                for o in app
                                    .ed
                                    .doc_mut()
                                    .objects
                                    .iter_mut()
                                    .filter(|o| sel.contains(&o.id))
                                {
                                    o.holes.clear();
                                }
                            }
                        });
                    }
                }
            }

            theme::section(ui, "Start point");
            if first.start.is_some() {
                theme::hint(ui, "Sewing begins near the flag on the canvas.");
                if ui
                    .button("Remove start point")
                    .on_hover_text("Let this object start where the previous one ended")
                    .clicked()
                {
                    app.ed.set_start_selected(None);
                }
            } else {
                theme::hint(ui, "Sewing begins where the previous object ended.");
                if ui
                    .button("Choose on the canvas")
                    .on_hover_text("Switch to the Start point tool (S) and click on the shape")
                    .clicked()
                {
                    app.tools.set_tool(Tool::Start);
                }
            }
            ui.horizontal(|ui| {
                ui.label("Or put it at");
                let cur = first.start.and_then(|s| {
                    let (lo, hi) = first.bounds()?;
                    let f =
                        |v: f64, a: f64, b: f64| if b - a > 1e-9 { (v - a) / (b - a) } else { 0.5 };
                    Some((f(s.x, lo.x, hi.x), f(s.y, lo.y, hi.y)))
                });
                if let Some((fx, fy)) = anchor_grid(ui, cur, "of the shape") {
                    app.ed.set_start_anchor_selected(fx, fy);
                }
            });

            theme::section(ui, "Thread");
            thread_palette(ui, app, Some(()));
            theme::section(ui, "Arrange");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                arrange_buttons(app, ui, true);
            });
            theme::section(ui, "Group and shapes");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                shape_buttons(app, ui);
            });
            if combinable(app) >= 2 {
                theme::hint(
                    ui,
                    "Subtract cuts the shapes on top out of the bottom one (the one sewn first).",
                );
            }
            if let Some((lo, hi)) = app.ed.selection_bounds() {
                theme::hint(
                    ui,
                    &format!("Size {:.1} × {:.1} mm", hi.x - lo.x, hi.y - lo.y),
                );
            }
        }
    }
}

/// Summary of a viewed machine file, with stitches per colour block.
fn machine_info(ui: &mut egui::Ui, p: &embcraft_core::Pattern) {
    theme::section(ui, "Machine file");
    egui::Grid::new("minfo")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            ui.label("Stitches");
            ui.label(p.stitch_count().to_string());
            ui.end_row();
            ui.label("Jumps");
            ui.label(p.jumps().to_string());
            ui.end_row();
            ui.label("Trims");
            ui.label(p.trims().to_string());
            ui.end_row();
            ui.label("Colour changes");
            ui.label(p.color_changes().to_string());
            ui.end_row();
            if let Some((lo, hi)) = p.bounds() {
                ui.label("Size");
                ui.label(format!("{:.1} × {:.1} mm", hi.x - lo.x, hi.y - lo.y));
                ui.end_row();
            }
            ui.label("Longest stitch");
            ui.label(format!("{:.2} mm", p.max_stitch_length()));
            ui.end_row();
        });
    theme::section(ui, "Colour blocks");
    let mut counts = vec![0usize];
    for s in &p.stitches {
        match s.kind {
            embcraft_core::StitchKind::Normal => {
                if let Some(c) = counts.last_mut() {
                    *c += 1;
                }
            }
            embcraft_core::StitchKind::ColorChange => counts.push(0),
            _ => {}
        }
    }
    for (i, n) in counts.iter().enumerate() {
        ui.horizontal(|ui| {
            let rgb = p.threads.get(i).map(|t| t.rgb).unwrap_or([0, 0, 0]);
            let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(r.center(), 7.0, Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
            ui.label(format!("Needle {}: {n} stitches", i + 1));
        });
    }
    theme::hint(
        ui,
        "DST files carry no thread colours, so these are placeholders. Choose real threads on the machine.",
    );
}

pub fn player_bar(app: &mut App, root: &mut egui::Ui) {
    if !app.player.open {
        return;
    }
    let len = app.shown_pattern().map_or(0, |p| p.stitches.len());
    egui::Panel::bottom("player")
        .frame(
            egui::Frame::side_top_panel(root.style()).inner_margin(egui::Margin::symmetric(10, 6)),
        )
        .show(root, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let p = &mut app.player;
                if icons::button(ui, Icon::ToStart, false, true, "Back to the first stitch")
                    .clicked()
                {
                    p.pos = 0;
                    p.playing = false;
                }
                if icons::button(ui, Icon::StepBack, false, true, "One stitch back").clicked() {
                    p.pos = p.pos.saturating_sub(1);
                    p.playing = false;
                }
                let (icon, tip) = if p.playing {
                    (Icon::Pause, "Pause")
                } else {
                    (Icon::Play, "Play")
                };
                if icons::button(ui, icon, p.playing, len > 0, tip).clicked() {
                    p.playing = !p.playing;
                }
                if icons::button(ui, Icon::StepForward, false, true, "One stitch forward").clicked()
                {
                    p.pos = (p.pos + 1).min(len);
                    p.playing = false;
                }
                if icons::button(ui, Icon::ToEnd, false, true, "Show every stitch").clicked() {
                    p.pos = len;
                    p.playing = false;
                }
                ui.add_space(10.0);
                ui.spacing_mut().slider_width = (ui.available_width() - 330.0).max(120.0);
                let mut pos = p.pos.min(len);
                if ui
                    .add(egui::Slider::new(&mut pos, 0..=len).show_value(false))
                    .changed()
                {
                    p.pos = pos;
                    p.playing = false;
                }
                ui.add_space(8.0);
                ui.label(format!("{pos} of {len}"));
                ui.add_space(8.0);
                egui::ComboBox::from_id_salt("speed")
                    .selected_text(format!("{:.0} per second", p.speed))
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for s in [20.0, 100.0, 200.0, 1000.0, 5000.0] {
                            ui.selectable_value(&mut p.speed, s, format!("{s:.0} per second"));
                        }
                    });
                if icons::button(
                    ui,
                    Icon::Close,
                    false,
                    true,
                    &crate::keys("Close the player (Ctrl+P)"),
                )
                .clicked()
                {
                    p.open = false;
                    p.playing = false;
                }
            });
        });
}

pub fn status_bar(app: &mut App, root: &mut egui::Ui) {
    egui::Panel::bottom("status").show(root, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            status_figures(app, ui);
            // the message takes whatever room is left, cut short rather than overlapping
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(&app.status).truncate());
            });
        });
    });
}

/// Right-hand side of the status bar, laid out right to left.
fn status_figures(app: &App, ui: &mut egui::Ui) {
    if let Some(c) = app.cursor_mm {
        ui.label(format!("x {:.1}  y {:.1} mm", c.x, c.y));
    }
    ui.separator();
    ui.label(format!("{:.0} %", app.view.scale * 100.0 / 3.78));
    let Some(p) = app.shown_pattern() else { return };
    let err = if app.machine.is_none() {
        app.preview.as_ref().and_then(|pr| pr.error.clone())
    } else {
        None
    };
    ui.separator();
    if let Some(e) = err {
        ui.colored_label(theme::WARN, e);
        return;
    }
    if p.max_stitch_length() > 12.1 {
        ui.colored_label(theme::WARN, "A stitch is longer than 12.1 mm");
        ui.separator();
    }
    if let Some((lo, hi)) = p.bounds() {
        let (hw, hh) = (app.hoop.0 / 2.0, app.hoop.1 / 2.0);
        if lo.x < -hw || hi.x > hw || lo.y < -hh || hi.y > hh {
            ui.colored_label(theme::WARN, "Outside the hoop");
            ui.separator();
        }
        ui.label(format!("{:.1} × {:.1} mm", hi.x - lo.x, hi.y - lo.y));
        ui.separator();
    }
    ui.label(format!("{} trims", p.trims()));
    ui.separator();
    ui.label(format!("{} colour changes", p.color_changes()));
    ui.separator();
    ui.label(format!("{} stitches", p.stitch_count()));
    ui.separator();
}

pub fn confirm_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(c) = app.confirm else { return };
    egui::Window::new("Unsaved changes")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("This design has changes that are not saved.");
            ui.horizontal(|ui| {
                if ui.button("Discard changes").clicked() {
                    app.confirm = None;
                    app.ed.mark_saved();
                    app.run_confirmed(c);
                }
                if ui.button("Keep editing").clicked() {
                    app.confirm = None;
                }
            });
        });
}
