//! EmbroideryCraft graphical editor.

// No console window behind the app on Windows (release builds).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backdrop;
mod canvas;
mod font_preview;
mod fonts;
mod help;
mod icons;
mod mockup;
mod panels;
mod product;
mod teamnames;
mod text;
mod theme;

use eframe::egui;
use egui_file_dialog::FileDialog;
use embcraft_core::{Pattern, Point, svg::to_svg};
use embcraft_design::BuildOptions;
use embcraft_doc::{Document, EditObject, Editor, StitchStyle, ToolController};
use std::path::{Path, PathBuf};

pub const PROJECT_EXT: &str = "embcraft";

/// Shortcut names as this computer writes them: "Ctrl+S" on Windows and Linux, "⌘S" on a Mac.
pub fn keys(text: &str) -> String {
    if cfg!(target_os = "macos") {
        text.replace("Ctrl+Shift+", "⇧⌘").replace("Ctrl+", "⌘")
    } else {
        text.to_string()
    }
}

/// Cached stitch preview for one document revision.
pub struct Preview {
    pub rev: u64,
    pub pattern: Pattern,
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    Open,
    SaveAs,
    ExportDst,
    ExportSvg,
    ViewMachineFile,
    Backdrop,
    TeamFolder,
    Mockup,
    ProductPhoto,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    New,
    Open,
    Quit,
}

/// Stitch-by-stitch playback of the preview (or of a machine file being viewed).
#[derive(Default)]
pub struct Player {
    pub open: bool,
    pub playing: bool,
    /// Number of pattern records shown.
    pub pos: usize,
    /// Records per second.
    pub speed: f32,
    carry: f32,
    /// Pattern length seen last frame, to notice when the design changes under the player.
    last_len: usize,
}

impl Player {
    pub fn toggle(&mut self, len: usize) {
        self.open = !self.open;
        self.playing = false;
        self.pos = len;
    }

    /// Keep showing the whole design when it changes while the player sits at the end
    /// (otherwise newly added stitches past the old end would stay hidden).
    fn follow(&mut self, len: usize) {
        if !self.playing && self.pos >= self.last_len {
            self.pos = len;
        }
        self.pos = self.pos.min(len);
        self.last_len = len;
    }

    fn advance(&mut self, dt: f32, len: usize) -> bool {
        if !self.open || !self.playing {
            return false;
        }
        if self.pos >= len {
            self.pos = 0;
        }
        self.carry += dt * self.speed;
        let step = self.carry.floor();
        self.carry -= step;
        self.pos = (self.pos + step as usize).min(len);
        if self.pos >= len {
            self.playing = false;
        }
        true
    }
}

/// A machine file opened for viewing (read-only; it replaces the design on the canvas).
pub struct MachineView {
    pub path: PathBuf,
    pub pattern: Pattern,
}

#[derive(Default)]
struct Script {
    screenshot: Option<PathBuf>,
    frames: u32,
    requested: bool,
    /// Rest the pointer here, and wait for tooltips before the screenshot.
    hover: Option<egui::Pos2>,
    started: Option<std::time::Instant>,
    /// Also save a mockup picture here (with `--product`, the product to show it on).
    mockup: Option<PathBuf>,
}

pub struct App {
    pub ed: Editor,
    pub tools: ToolController,
    pub view: canvas::View,
    pub show_stitches: bool,
    pub show_grid: bool,
    pub show_hoop: bool,
    /// Hoop size in mm (width, height), centred on the origin.
    pub hoop: (f64, f64),
    pub preview: Option<Preview>,
    pub file: Option<PathBuf>,
    pub status: String,
    pub cursor_mm: Option<Point>,
    /// The primary button went down on the canvas and has not been released yet.
    pub canvas_press: bool,
    pub confirm: Option<Confirm>,
    pub fit_pending: bool,
    /// Zero-movement jumps written for each trim in exported DST files.
    pub trim_jumps: u8,
    pub player: Player,
    pub machine: Option<MachineView>,
    /// The lettering window, while open.
    pub text: Option<text::TextDialog>,
    /// Installed fonts, listed the first time the lettering window opens.
    pub fonts: Option<Vec<embcraft_text::FontFace>>,
    /// EmbroideryCraft fonts in the fonts folder, `(name, path)`.
    pub ec_fonts: Vec<(String, PathBuf)>,
    /// Font names drawn in their own font, for the font lists.
    pub previews: font_preview::FontPreviews,
    pub font_maker: Option<fonts::FontMaker>,
    /// A font being edited: the canvas shows one of its characters instead of the design.
    pub font_edit: Option<fonts::FontEdit>,
    /// The picture under the design, loaded for display.
    pub backdrop_tex: Option<backdrop::Texture>,
    pub team: Option<teamnames::TeamNames>,
    /// The product under the design, drawn for display; the picker window and its pictures.
    pub product_tex: Option<product::Texture>,
    pub product_picker: bool,
    /// The About window is open.
    pub about: bool,
    pub product_thumbs: Vec<egui::TextureHandle>,
    /// Draw stitches as shaded thread.
    pub realistic: bool,
    /// Lettering settings shown as a tab of the right panel instead of a window.
    pub text_docked: bool,
    /// With docked lettering open: show its tab (else Properties).
    pub lettering_tab: bool,
    dialog: FileDialog,
    pending: Option<Pending>,
    script: Script,
    quit_ok: bool,
    /// The window title last sent.
    shown_title: String,
    /// Tooltip text for the object under the pointer: (object, document revision, text).
    pub hover_info: Option<(embcraft_doc::ObjectId, u64, String)>,
}

struct Args {
    open: Option<PathBuf>,
    /// Select the lettering with this index and open its window: "edit" or team names given
    /// as "a,b,c" (screenshot tests).
    lettering: Option<(usize, String)>,
    /// Show the lettering settings docked in the right panel (screenshot tests).
    dock: bool,
    /// Rest the pointer at "x,y" before the screenshot (tooltip tests).
    hover: Option<egui::Pos2>,
    /// Save a mockup picture with the screenshot; show the design on this product first (its
    /// position in the picker), and open the picker (screenshot tests).
    mockup: Option<PathBuf>,
    product: Option<usize>,
    picker: bool,
    demo: bool,
    screenshot: Option<PathBuf>,
    select_all: bool,
    no_stitches: bool,
    /// Open the stitch player at this fraction of the design (screenshot tests).
    player: Option<f32>,
    /// Start with this tool active, by its label (screenshot tests).
    tool: Option<String>,
    /// Open the lettering window with this text (screenshot tests).
    text: Option<String>,
    /// Open this EmbroideryCraft font in the font editor (screenshot tests).
    edit_font: Option<PathBuf>,
    /// Open the font maker window (screenshot tests).
    font_maker: bool,
    size: (f32, f32),
}

fn parse_args() -> Args {
    let mut a = Args {
        open: None,
        demo: false,
        screenshot: None,
        select_all: false,
        no_stitches: false,
        player: None,
        tool: None,
        text: None,
        edit_font: None,
        font_maker: false,
        lettering: None,
        hover: None,
        mockup: None,
        product: None,
        picker: false,
        dock: false,
        size: (1360.0, 860.0),
    };
    let mut it = std::env::args().skip(1);
    while let Some(s) = it.next() {
        match s.as_str() {
            "--demo" => a.demo = true,
            "--select-all" => a.select_all = true,
            "--no-stitches" => a.no_stitches = true,
            "--screenshot" => a.screenshot = it.next().map(PathBuf::from),
            "--player" => a.player = it.next().and_then(|v| v.parse().ok()),
            "--tool" => a.tool = it.next(),
            "--text" => a.text = it.next(),
            "--edit-font" => a.edit_font = it.next().map(PathBuf::from),
            "--font-maker" => a.font_maker = true,
            "--dock" => a.dock = true,
            "--mockup" => a.mockup = it.next().map(PathBuf::from),
            "--product" => a.product = it.next().and_then(|v| v.parse().ok()),
            "--picker" => a.picker = true,
            "--hover" => {
                a.hover = it.next().and_then(|v| {
                    let (x, y) = v.split_once(',')?;
                    Some(egui::pos2(x.parse().ok()?, y.parse().ok()?))
                })
            }
            "--lettering" => {
                let n = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                a.lettering = it.next().map(|w| (n, w));
            }
            "--size" => {
                if let Some((w, h)) = it.next().as_deref().and_then(|v| v.split_once('x')) {
                    if let (Ok(w), Ok(h)) = (w.parse(), h.parse()) {
                        a.size = (w, h);
                    }
                }
            }
            other if !other.starts_with("--") => a.open = Some(PathBuf::from(other)),
            _ => {}
        }
    }
    a
}

fn demo_document() -> Document {
    let mut d = Document::new("Demo");
    let mut ed = Editor::new(d.clone());
    let circle = |cx: f64, cy: f64, r: f64| embcraft_doc::ellipse_points(Point::new(cx, cy), r, r);
    ed.add_object(
        "Ring",
        circle(-30.0, -10.0, 22.0),
        true,
        StitchStyle::fill(),
        0,
    );
    if let Some(o) = ed.doc_mut().objects.last_mut() {
        o.holes.push(circle(-30.0, -10.0, 10.0));
        o.start = Some(Point::new(-50.0, -10.0));
    }
    let wave: Vec<Point> = (0..=40)
        .map(|i| {
            let t = i as f64 / 40.0;
            Point::new(
                5.0 + 55.0 * t,
                -10.0 + 12.0 * (t * std::f64::consts::TAU).sin(),
            )
        })
        .collect();
    ed.add_object("Wave", wave, false, StitchStyle::satin(4.0), 1);
    let tri = vec![
        Point::new(-20.0, 30.0),
        Point::new(20.0, 30.0),
        Point::new(0.0, 58.0),
    ];
    ed.add_object(
        "Triangle",
        tri,
        true,
        StitchStyle::Run {
            params: embcraft_stitch::RunParams {
                triple: true,
                ..Default::default()
            },
        },
        4,
    );
    ed.clear_selection();
    d = ed.doc().clone();
    d
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, args: Args) -> Self {
        theme::apply(&cc.egui_ctx);
        install_cjk_ui_fonts(&cc.egui_ctx);
        let mut ed = Editor::default();
        let mut file = None;
        let mut status =
            crate::keys("Pick a tool on the left and draw a shape. Ctrl+E exports a DST file.");
        let machine_file = args
            .open
            .as_deref()
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dst")));
        if args.demo {
            ed.replace_document(demo_document());
        } else if machine_file.is_some() {
            // read after construction, below
        } else if let Some(p) = &args.open {
            match std::fs::read_to_string(p)
                .map_err(|e| e.to_string())
                .and_then(|t| Document::from_json(&t).map_err(|e| e.to_string()))
            {
                Ok(doc) => {
                    ed.replace_document(doc);
                    file = Some(p.clone());
                    status = format!("Opened {}", p.display());
                }
                Err(e) => status = format!("Could not open {}: {e}", p.display()),
            }
        }
        if args.select_all {
            ed.select_all();
        }
        let mut tools = ToolController::default();
        tools.defaults.style = StitchStyle::fill();
        if let Some(t) = embcraft_doc::Tool::ALL.into_iter().find(|t| {
            args.tool
                .as_deref()
                .is_some_and(|n| t.label().eq_ignore_ascii_case(n))
        }) {
            tools.set_tool(t);
        }
        let mut app = Self {
            ed,
            tools,
            view: canvas::View::default(),
            show_stitches: !args.no_stitches,
            show_grid: true,
            show_hoop: true,
            hoop: (200.0, 200.0),
            preview: None,
            file,
            status,
            cursor_mm: None,
            canvas_press: false,
            confirm: None,
            fit_pending: true,
            trim_jumps: embcraft_dst::WriteOptions::default().trim_jumps,
            player: Player {
                speed: 200.0,
                ..Player::default()
            },
            machine: None,
            text: None,
            fonts: None,
            ec_fonts: embcraft_doc::font::installed_fonts(),
            previews: Default::default(),
            font_maker: None,
            font_edit: None,
            backdrop_tex: None,
            team: None,
            product_tex: None,
            product_picker: false,
            about: false,
            product_thumbs: Vec::new(),
            realistic: false,
            text_docked: text::load_docked(),
            lettering_tab: false,
            dialog: FileDialog::new(),
            pending: None,
            script: Script {
                hover: args.hover,
                started: None,
                screenshot: args.screenshot,
                mockup: args.mockup,
                ..Script::default()
            },
            quit_ok: false,
            shown_title: String::new(),
            hover_info: None,
        };
        if let Some(p) = machine_file {
            app.read_machine_file(p);
        }
        if let Some(t) = args.text {
            let fonts = app.fonts.get_or_insert_with(embcraft_text::system_fonts);
            app.ec_fonts = embcraft_doc::font::installed_fonts();
            let mut d = text::TextDialog::new(Point::new(-60.0, 45.0), fonts, &app.ec_fonts);
            d.text = t;
            d.opts.cap_height = 22.0;
            app.text = Some(d);
        }
        if let Some(k) = args
            .product
            .and_then(|i| embcraft_doc::product::ProductKind::ALL.get(i))
        {
            let at = app
                .ed
                .doc()
                .to_design()
                .bounds()
                .map_or(Point::ZERO, |(lo, hi)| (lo + hi) * 0.5);
            let v = embcraft_doc::product::ProductView::new(*k, at);
            app.ed.set_product("script", Some(v));
            app.realistic = true;
        }
        app.product_picker = args.picker;
        if args.dock {
            app.text_docked = true;
            app.lettering_tab = true;
        }
        if args.font_maker {
            app.font_maker = Some(fonts::FontMaker::default());
        }
        if let Some((n, what)) = args.lettering {
            if let Some(&b) = app.ed.doc().letterings.keys().nth(n) {
                let ids = app.ed.doc().lettering_objects(b);
                app.ed.select_set(ids);
                if what == "edit" {
                    text::edit_selected(&mut app);
                } else {
                    teamnames::open(&mut app);
                    if let Some(t) = &mut app.team {
                        t.names = what.replace(',', "\n");
                    }
                }
            }
        }
        if let Some(p) = args.edit_font {
            match embcraft_doc::font::EcFont::load(&p) {
                Ok(f) => fonts::begin_edit(&mut app, f, p),
                Err(e) => app.status = e.to_string(),
            }
        }
        if let Some(f) = args.player {
            app.refresh_preview();
            let len = app.shown_pattern().map_or(0, |p| p.stitches.len());
            app.player.toggle(len);
            app.player.pos = (len as f32 * f.clamp(0.0, 1.0)) as usize;
        }
        app
    }

    pub fn title(&self) -> String {
        if let Some(fe) = &self.font_edit {
            return format!(
                "{}{}{} — EmbroideryCraft font editor",
                fe.font.name,
                fe.ch.map(|c| format!(": {c}")).unwrap_or_default(),
                if self.ed.is_dirty() { " *" } else { "" }
            );
        }
        let name = self
            .file
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        format!(
            "{}{} — EmbroideryCraft",
            name,
            if self.ed.is_dirty() { " *" } else { "" }
        )
    }

    /// Rebuild the stitch preview if the document changed.
    pub fn refresh_preview(&mut self) {
        if self
            .preview
            .as_ref()
            .is_some_and(|p| p.rev == self.ed.revision())
        {
            return;
        }
        self.ed.cache_satin_regions();
        let design = self.ed.doc().to_design();
        // design coordinates, to draw on the canvas (machine files are relocated on export)
        let opts = BuildOptions {
            center: false,
            relocate: false,
            ..BuildOptions::default()
        };
        let rev = self.ed.revision();
        self.preview = Some(match design.to_pattern(&opts) {
            Ok(pattern) => Preview {
                rev,
                pattern,
                error: None,
            },
            Err(e) => Preview {
                rev,
                pattern: Pattern::default(),
                error: Some(e.to_string()),
            },
        });
    }

    // ---- file actions ----------------------------------------------------------------------

    /// While a font is being edited, project actions wait until editing is finished.
    fn font_busy(&mut self) -> bool {
        if self.font_edit.is_some() {
            self.status =
                "A font is being edited: click Finish editing in the right panel first.".into();
        }
        self.font_edit.is_some()
    }

    pub fn request(&mut self, c: Confirm) {
        if c != Confirm::Quit && self.font_busy() {
            return;
        }
        if self.ed.is_dirty() {
            self.confirm = Some(c);
        } else {
            self.run_confirmed(c);
        }
    }

    pub fn run_confirmed(&mut self, c: Confirm) {
        match c {
            Confirm::New => {
                self.ed.replace_document(Document::default());
                self.machine = None;
                self.file = None;
                self.fit_pending = true;
                self.status = "New design".into();
            }
            Confirm::Open => self.open_dialog(Pending::Open),
            Confirm::Quit => self.quit_ok = true,
        }
    }

    fn open_dialog(&mut self, p: Pending) {
        let mut d = FileDialog::new();
        d = match p {
            Pending::Open => d
                .add_file_filter_extensions("EmbroideryCraft project", vec![PROJECT_EXT])
                .default_file_filter("EmbroideryCraft project"),
            Pending::SaveAs => d
                .add_save_extension("EmbroideryCraft project", PROJECT_EXT)
                .default_save_extension("EmbroideryCraft project")
                .default_file_name("design.embcraft"),
            Pending::ExportDst => d
                .add_save_extension("Tajima DST", "dst")
                .default_save_extension("Tajima DST")
                .default_file_name("design.dst"),
            Pending::ExportSvg => d
                .add_save_extension("SVG preview", "svg")
                .default_save_extension("SVG preview")
                .default_file_name("preview.svg"),
            Pending::ViewMachineFile => d
                .add_file_filter_extensions("Tajima DST", vec!["dst", "DST"])
                .default_file_filter("Tajima DST"),
            Pending::Mockup => d
                .add_save_extension("PNG picture", "png")
                .default_save_extension("PNG picture")
                .default_file_name("mockup.png"),
            Pending::Backdrop | Pending::ProductPhoto => d
                .add_file_filter_extensions(
                    "Pictures",
                    vec![
                        "png", "jpg", "jpeg", "bmp", "webp", "gif", "PNG", "JPG", "JPEG",
                    ],
                )
                .default_file_filter("Pictures"),
            Pending::TeamFolder => d,
        };
        if p == Pending::TeamFolder {
            d.pick_directory();
        } else if matches!(
            p,
            Pending::Open | Pending::ViewMachineFile | Pending::Backdrop | Pending::ProductPhoto
        ) {
            d.pick_file();
        } else {
            d.save_file();
        }
        self.dialog = d;
        self.pending = Some(p);
    }

    pub fn open_file(&mut self) {
        self.request(Confirm::Open);
    }
    pub fn save_as(&mut self) {
        self.open_dialog(Pending::SaveAs);
    }
    pub fn export_dst(&mut self) {
        if !self.font_busy() {
            self.open_dialog(Pending::ExportDst);
        }
    }
    pub fn export_svg(&mut self) {
        self.open_dialog(Pending::ExportSvg);
    }
    pub fn view_machine_file(&mut self) {
        self.open_dialog(Pending::ViewMachineFile);
    }
    pub fn pick_backdrop(&mut self) {
        self.open_dialog(Pending::Backdrop);
    }
    pub fn pick_product_photo(&mut self) {
        self.open_dialog(Pending::ProductPhoto);
    }
    pub fn export_mockup(&mut self) {
        if !self.font_busy() {
            self.open_dialog(Pending::Mockup);
        }
    }
    pub fn pick_team_folder(&mut self) {
        self.open_dialog(Pending::TeamFolder);
    }

    /// The pattern the canvas and stitch player show: a viewed machine file, else the preview.
    pub fn shown_pattern(&self) -> Option<&Pattern> {
        match &self.machine {
            Some(m) => Some(&m.pattern),
            None => self.preview.as_ref().map(|p| &p.pattern),
        }
    }

    pub fn close_machine_file(&mut self) {
        self.machine = None;
        self.player.open = false;
        self.fit_pending = true;
        self.status = "Back to the design".into();
    }

    fn read_machine_file(&mut self, p: &Path) {
        match std::fs::read(p)
            .map_err(|e| e.to_string())
            .and_then(|b| embcraft_dst::read(&b).map_err(|e| e.to_string()))
        {
            Ok(pattern) => {
                self.status = format!(
                    "Viewing {}: {} stitches, {} colour changes, {} trims",
                    p.display(),
                    pattern.stitch_count(),
                    pattern.color_changes(),
                    pattern.trims()
                );
                self.player.pos = pattern.stitches.len();
                self.machine = Some(MachineView {
                    path: p.to_path_buf(),
                    pattern,
                });
                self.fit_pending = true;
            }
            Err(e) => self.status = format!("Could not read {}: {e}", p.display()),
        }
    }

    pub fn save(&mut self) {
        if self.font_edit.is_some() {
            fonts::store_glyph(self);
            return;
        }
        match self.file.clone() {
            Some(p) => self.write_project(&p),
            None => self.save_as(),
        }
    }

    fn write_project(&mut self, p: &Path) {
        match self
            .ed
            .doc()
            .to_json()
            .map_err(|e| e.to_string())
            .and_then(|j| std::fs::write(p, j).map_err(|e| e.to_string()))
        {
            Ok(()) => {
                self.file = Some(p.to_path_buf());
                self.ed.mark_saved();
                self.status = format!("Saved {}", p.display());
            }
            Err(e) => self.status = format!("Could not save {}: {e}", p.display()),
        }
    }

    fn read_project(&mut self, p: &Path) {
        match std::fs::read_to_string(p)
            .map_err(|e| e.to_string())
            .and_then(|t| Document::from_json(&t).map_err(|e| e.to_string()))
        {
            Ok(doc) => {
                self.ed.replace_document(doc);
                self.machine = None;
                self.file = Some(p.to_path_buf());
                self.fit_pending = true;
                self.status = format!("Opened {}", p.display());
            }
            Err(e) => self.status = format!("Could not open {}: {e}", p.display()),
        }
    }

    fn export(&mut self, p: &Path, dst: bool) {
        self.refresh_preview();
        let Some(pre) = &self.preview else { return };
        if let Some(e) = &pre.error {
            self.status = format!("Cannot export: {e}");
            return;
        }
        if pre.pattern.stitch_count() == 0 {
            self.status = "Nothing to export: the design has no stitches".into();
            return;
        }
        let res: Result<(), String> = if dst {
            let opts = embcraft_dst::WriteOptions {
                trim_jumps: self.trim_jumps,
                ..embcraft_dst::WriteOptions::default()
            };
            // the machine's origin is the chosen design start (the hoop centre by default)
            let build = BuildOptions {
                center: false,
                ..BuildOptions::default()
            };
            self.ed
                .doc()
                .to_design()
                .to_pattern(&build)
                .map_err(|e| e.to_string())
                .and_then(|p| embcraft_dst::write(&p, &opts).map_err(|e| e.to_string()))
                .and_then(|b| std::fs::write(p, b).map_err(|e| e.to_string()))
        } else {
            std::fs::write(p, to_svg(&pre.pattern, 0.35)).map_err(|e| e.to_string())
        };
        self.status = match res {
            Ok(()) => format!(
                "Exported {} ({} stitches)",
                p.display(),
                pre.pattern.stitch_count()
            ),
            Err(e) => format!("Export failed: {e}"),
        };
    }

    fn handle_dialog(&mut self, ctx: &egui::Context) {
        self.dialog.update(ctx);
        if let Some(path) = self.dialog.take_picked() {
            match self.pending.take() {
                Some(Pending::Open) => self.read_project(&path),
                Some(Pending::SaveAs) => self.write_project(&path),
                Some(Pending::ExportDst) => self.export(&path, true),
                Some(Pending::ExportSvg) => self.export(&path, false),
                Some(Pending::ViewMachineFile) => self.read_machine_file(&path),
                Some(Pending::Backdrop) => backdrop::add(self, &path),
                Some(Pending::ProductPhoto) => product::add_photo(self, &path),
                Some(Pending::Mockup) => {
                    self.status = match product::export(self, &path) {
                        Ok(m) | Err(m) => m,
                    }
                }
                Some(Pending::TeamFolder) => {
                    if let Some(t) = &mut self.team {
                        t.folder = Some(path.to_path_buf());
                    }
                }
                None => {}
            }
        }
    }

    pub fn selected_first(&self) -> Option<&EditObject> {
        self.ed.selected_objects().next()
    }

    fn run_script(&mut self, ctx: &egui::Context) {
        let Some(path) = self.script.screenshot.clone() else {
            return;
        };
        self.script.frames += 1;
        let started = *self
            .script
            .started
            .get_or_insert_with(std::time::Instant::now);
        let waited = self.script.hover.is_none() || started.elapsed().as_secs_f32() > 2.0;
        // with a resting pointer, let the app go idle as it does in use: tooltips must then
        // appear on egui's own delayed repaint
        if self.script.hover.is_some() && !waited {
            ctx.request_repaint_after_secs(2.2);
        }
        if self.script.frames > 8 && waited && !self.previews.busy() && !self.script.requested {
            self.script.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            if let Some(m) = self.script.mockup.clone() {
                self.status = match product::export(self, &m) {
                    Ok(s) | Err(s) => s,
                };
                eprintln!("{}", self.status);
            }
            let (w, h) = (img.size[0] as u32, img.size[1] as u32);
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(w, h, bytes) {
                let _ = buf.save(&path);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.script.hover.is_none() || self.script.requested {
            ctx.request_repaint();
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(p) = self.script.hover {
            if self.script.frames < 3 {
                raw_input.events.push(egui::Event::PointerMoved(p));
            }
        }
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        // Only when it changes: every viewport command makes another frame, so sending it each
        // frame kept the app redrawing nonstop (and the window manager busy).
        let title = self.title();
        if title != self.shown_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.shown_title = title;
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.quit_ok && self.ed.is_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(Confirm::Quit);
        }
        if self.quit_ok {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if !ctx.input(|i| i.pointer.any_down()) {
            self.refresh_preview();
        }
        let len = self.shown_pattern().map_or(0, |p| p.stitches.len());
        self.player.follow(len);
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        if self.player.advance(dt, len) {
            ctx.request_repaint();
        }
        panels::shortcuts(self, ctx);
        panels::top_menu(self, root);
        panels::toolbar(self, root);
        panels::status_bar(self, root);
        panels::player_bar(self, root);
        panels::left_panel(self, root);
        panels::right_panel(self, root);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(root, |ui| canvas::show(self, ui));
        text::show(self, ctx);
        teamnames::show(self, ctx);
        product::picker(self, ctx);
        help::about(self, ctx);
        fonts::show_maker(self, ctx);
        panels::confirm_dialog(self, ctx);
        self.handle_dialog(ctx);
        // An edit made this frame (e.g. a shape finished on mouse release) is not in the preview
        // built at the top of the frame: ask for one more frame so it shows without waiting for
        // the mouse to move.
        if self
            .preview
            .as_ref()
            .is_none_or(|p| p.rev != self.ed.revision())
        {
            ctx.request_repaint();
        }
        self.run_script(ctx);
    }
}

/// Let the interface show Kanji, Kana and Hangul (egui's own fonts have none), using fonts
/// installed on the computer as fallbacks.
fn install_cjk_ui_fonts(ctx: &egui::Context) {
    let extra = embcraft_text::ui_fallback_fonts();
    if extra.is_empty() {
        return;
    }
    let mut defs = egui::FontDefinitions::default();
    for (i, (_, data, index)) in extra.into_iter().enumerate() {
        let key = format!("cjk-{i}");
        let mut fd = egui::FontData::from_owned(data);
        fd.index = index;
        defs.font_data.insert(key.clone(), std::sync::Arc::new(fd));
        for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            defs.families.entry(fam).or_default().push(key.clone());
        }
    }
    ctx.set_fonts(defs);
}

fn main() -> eframe::Result {
    let args = parse_args();
    let (w, h) = args.size;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([w, h])
            .with_min_inner_size([900.0, 600.0])
            .with_title("EmbroideryCraft"),
        ..Default::default()
    };
    eframe::run_native(
        "EmbroideryCraft",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc, args)))),
    )
}

#[cfg(test)]
mod tests {
    use super::Player;

    #[test]
    fn player_at_the_end_follows_a_growing_design() {
        let mut p = Player::default();
        p.toggle(100);
        p.follow(100);
        p.follow(250); // a fill was added while the player was open
        assert_eq!(p.pos, 250);
        p.pos = 40; // scrubbed back: stays put when the design changes
        p.follow(250);
        p.follow(300);
        assert_eq!(p.pos, 40);
        p.follow(10); // design shrank below the position
        assert_eq!(p.pos, 10);
    }
}
