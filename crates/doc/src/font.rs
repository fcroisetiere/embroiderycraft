//! EmbroideryCraft fonts: lettering already worked out for stitching.
//!
//! A font is made once from a computer font (TrueType/OpenType), or drawn from scratch, and kept
//! in a `.ecfont` file. Every character is a small design: auto-satin shapes whose satin regions
//! (stitch directions) are stored, so lettering is instant, sews the same every time, and each
//! character can be edited by hand in the editor. Coordinates are in em (1.0 = the font size),
//! y down, with the baseline at y = 0 and the character starting at x = 0.

use crate::model::{EditObject, StitchStyle};
use embcraft_core::Point;
use embcraft_text::{FontMetrics, TextOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FONT_FORMAT: &str = "embroiderycraft-font";
pub const FONT_VERSION: u32 = 1;
pub const FONT_EXT: &str = "ecfont";
/// Characters are converted (and edited) at this size in millimetres, so the satin converter's
/// thresholds (0.1 mm cells, 1 mm² specks) suit real letter sizes.
pub const REFERENCE_MM: f64 = 30.0;

#[derive(Debug, thiserror::Error)]
pub enum FontError {
    #[error("{0}")]
    Io(String),
    #[error("not an EmbroideryCraft font: {0}")]
    Invalid(String),
    #[error("font file version {0} is newer than this program understands ({FONT_VERSION})")]
    TooNew(u32),
    #[error(transparent)]
    Text(#[from] embcraft_text::TextError),
}

/// The computer font characters are converted from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FontSource {
    pub family: String,
    pub style: String,
    pub path: PathBuf,
    #[serde(default)]
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EcGlyph {
    /// Distance to the next character, in em.
    pub advance: f64,
    pub objects: Vec<EditObject>,
    /// Changed by hand after conversion (kept when converting again).
    #[serde(default)]
    pub edited: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EcFont {
    pub format: String,
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub source: Option<FontSource>,
    pub ascent: f64,
    pub descent: f64,
    pub cap_height: f64,
    pub glyphs: BTreeMap<char, EcGlyph>,
}

/// What a conversion did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConvertReport {
    pub added: Vec<char>,
    /// Not in the computer font.
    pub missing: Vec<char>,
    /// Already in the EmbroideryCraft font (left as they were).
    pub kept: Vec<char>,
}

/// Character sets offered when making a font.
pub fn katakana() -> String {
    ('\u{30A1}'..='\u{30FA}').chain(['ー', '・']).collect()
}
pub fn hiragana() -> String {
    ('\u{3041}'..='\u{3096}').collect()
}
pub fn latin() -> String {
    ('!'..='~').collect()
}
/// Kanji and Hangul often embroidered in martial arts (dojo names, styles, ranks, values).
pub fn martial_arts() -> String {
    "空手道柔剣合気武術拳法少林寺流館会士心技体礼忍勇誠義仁智信段級師範先生帯黒白赤青茶道場極真剛松濤和糸東正\
     태권도합기유술검무예관사범단급띠"
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

impl EcFont {
    pub fn new(name: &str, metrics: FontMetrics, source: Option<FontSource>) -> Self {
        Self {
            format: FONT_FORMAT.into(),
            version: FONT_VERSION,
            name: name.to_string(),
            source,
            ascent: metrics.ascent,
            descent: metrics.descent,
            cap_height: metrics.cap_height,
            glyphs: BTreeMap::new(),
        }
    }

    /// An empty font for drawing characters by hand (typical metrics: 0.8 em above the
    /// baseline, 0.2 below).
    pub fn blank(name: &str) -> Self {
        Self::new(
            name,
            FontMetrics {
                ascent: 0.8,
                descent: 0.2,
                cap_height: 0.7,
            },
            None,
        )
    }

    pub fn metrics(&self) -> FontMetrics {
        FontMetrics {
            ascent: self.ascent,
            descent: self.descent,
            cap_height: self.cap_height,
        }
    }

    /// Convert the characters in `chars` from the computer font `data` (skipping those already
    /// present unless `replace` is set; hand-edited characters are always kept).
    pub fn convert(
        &mut self,
        data: &[u8],
        index: u32,
        chars: &str,
        replace: bool,
    ) -> Result<ConvertReport, FontError> {
        let mut report = ConvertReport::default();
        let mut seen = std::collections::HashSet::new();
        for ch in chars
            .chars()
            .filter(|c| !c.is_whitespace() && seen.insert(*c))
        {
            if let Some(g) = self.glyphs.get(&ch) {
                if g.edited || !replace {
                    report.kept.push(ch);
                    continue;
                }
            }
            match embcraft_text::glyph(data, index, ch)? {
                None => report.missing.push(ch),
                Some(g) => {
                    self.glyphs.insert(ch, convert_glyph(ch, &g));
                    report.added.push(ch);
                }
            }
        }
        Ok(report)
    }

    /// Objects for `text` in millimetres (first baseline, or top of the first column when
    /// vertical, at `origin`), and the characters the font does not have.
    pub fn layout(
        &self,
        text: &str,
        origin: Point,
        opts: &TextOptions,
    ) -> (Vec<EditObject>, Vec<char>) {
        let kerner = opts.auto_kern.then(|| {
            embcraft_text::Kerner::new(text.chars(), |c| {
                let g = self.glyphs.get(&c)?;
                let shapes = g
                    .objects
                    .iter()
                    .map(|o| (o.points.clone(), Vec::new()))
                    .collect();
                Some((g.advance, shapes))
            })
        });
        let (placed, missing) = embcraft_text::arrange(
            text,
            &self.metrics(),
            |c| {
                self.glyphs
                    .get(&c)
                    .map(|g| g.advance)
                    .or(c.is_whitespace().then_some(0.5))
            },
            |a, b| kerner.as_ref().map_or(0.0, |k| k.kern(a, b)),
            origin,
            opts,
        );
        let mut out = Vec::new();
        for pl in placed {
            let Some(g) = self.glyphs.get(&pl.ch) else {
                continue;
            };
            for o in &g.objects {
                let mut o = o.clone();
                o.map_points(|p| pl.map(p));
                if let StitchStyle::Satin { width, .. } = &mut o.style {
                    *width *= pl.scale;
                }
                out.push(o);
            }
        }
        (out, missing)
    }

    /// One character as objects at the editing size ([`REFERENCE_MM`] per em), baseline at y = 0.
    pub fn glyph_for_editing(&self, ch: char) -> Vec<EditObject> {
        let opts = TextOptions {
            cap_height: REFERENCE_MM,
            size_is_em: true,
            ..TextOptions::default()
        };
        self.layout(&ch.to_string(), Point::ZERO, &opts).0
    }

    /// Store edited objects (at the editing size) as character `ch`.
    pub fn store_edited(&mut self, ch: char, objects: &[EditObject], advance: f64) {
        let objects = objects
            .iter()
            .filter(|o| o.visible)
            .cloned()
            .map(|mut o| {
                o.map_points(|p| p * (1.0 / REFERENCE_MM));
                if let StitchStyle::Satin { width, .. } = &mut o.style {
                    *width /= REFERENCE_MM;
                }
                o.start = None;
                o
            })
            .collect();
        self.glyphs.insert(
            ch,
            EcGlyph {
                advance,
                objects,
                edited: true,
            },
        );
    }

    pub fn to_json(&self) -> Result<String, FontError> {
        serde_json::to_string(self).map_err(|e| FontError::Invalid(e.to_string()))
    }

    pub fn from_json(s: &str) -> Result<Self, FontError> {
        let f: EcFont = serde_json::from_str(s).map_err(|e| FontError::Invalid(e.to_string()))?;
        if f.format != FONT_FORMAT {
            return Err(FontError::Invalid(format!(
                "unexpected format '{}'",
                f.format
            )));
        }
        if f.version > FONT_VERSION {
            return Err(FontError::TooNew(f.version));
        }
        Ok(f)
    }

    pub fn load(path: &Path) -> Result<Self, FontError> {
        let s = std::fs::read_to_string(path)
            .map_err(|e| FontError::Io(format!("{}: {e}", path.display())))?;
        Self::from_json(&s)
    }

    /// Write the font, through a temporary file so a crash never leaves half a font.
    pub fn save(&self, path: &Path) -> Result<(), FontError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| FontError::Io(format!("{}: {e}", dir.display())))?;
        }
        let tmp = path.with_extension("ecfont.tmp");
        std::fs::write(&tmp, self.to_json()?)
            .map_err(|e| FontError::Io(format!("{}: {e}", tmp.display())))?;
        std::fs::rename(&tmp, path).map_err(|e| FontError::Io(format!("{}: {e}", path.display())))
    }

    /// Where this font is kept in the fonts folder.
    pub fn default_path(&self) -> PathBuf {
        let safe: String = self
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        fonts_dir().join(format!("{}.{FONT_EXT}", safe.trim()))
    }
}

/// One converted character: each shape becomes an auto-satin object with its regions worked out
/// at the reference size and stored.
fn convert_glyph(ch: char, g: &embcraft_text::Glyph) -> EcGlyph {
    let objects = g
        .shapes
        .iter()
        .enumerate()
        .map(|(i, (outer, holes))| {
            let mm = |p: &Point| *p * REFERENCE_MM;
            let mut o = EditObject {
                id: i as u64 + 1,
                name: format!("{ch} {}", i + 1),
                thread: 0,
                points: outer.iter().map(mm).collect(),
                closed: true,
                holes: holes.iter().map(|h| h.iter().map(mm).collect()).collect(),
                style: StitchStyle::auto_satin(),
                visible: true,
                start: None,
                lettering: None,
                group: None,
            };
            let mut rings = vec![o.points.clone()];
            rings.extend(o.holes.iter().cloned());
            o.set_satin_regions(embcraft_stitch::satin_regions(&rings));
            o.map_points(|p| p * (1.0 / REFERENCE_MM));
            o
        })
        .collect();
    EcGlyph {
        advance: g.advance,
        objects,
        edited: false,
    }
}

/// The folder EmbroideryCraft fonts are kept in: `$EMBROIDERYCRAFT_FONTS`, else
/// `%APPDATA%\EmbroideryCraft\fonts` on Windows, `~/Library/Application Support/EmbroideryCraft/fonts`
/// on macOS, else `$XDG_DATA_HOME/embroiderycraft/fonts` or `~/.local/share/embroiderycraft/fonts`.
pub fn fonts_dir() -> PathBuf {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(p) = env("EMBROIDERYCRAFT_FONTS") {
        return p;
    }
    if cfg!(windows) {
        if let Some(p) = env("APPDATA") {
            return p.join("EmbroideryCraft").join("fonts");
        }
    }
    if cfg!(target_os = "macos") {
        if let Some(h) = env("HOME") {
            return h.join("Library/Application Support/EmbroideryCraft/fonts");
        }
    }
    if let Some(p) = env("XDG_DATA_HOME") {
        return p.join("embroiderycraft").join("fonts");
    }
    env("HOME")
        .map(|h| h.join(".local/share/embroiderycraft/fonts"))
        .unwrap_or_else(|| PathBuf::from("fonts"))
}

/// EmbroideryCraft fonts in the fonts folder: `(name, path)`, sorted by name. Unreadable files
/// are skipped.
pub fn installed_fonts() -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(fonts_dir()) else {
        return Vec::new();
    };
    let mut v: Vec<(String, PathBuf)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(FONT_EXT))
        .filter_map(|p| EcFont::load(&p).ok().map(|f| (f.name, p)))
        .collect();
    v.sort_by_key(|f| f.0.to_lowercase());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cjk_font() -> Option<(Vec<u8>, u32)> {
        embcraft_text::system_fonts()
            .into_iter()
            .filter(|f| f.family.contains("CJK"))
            .find_map(|f| {
                let data = std::fs::read(&f.path).ok()?;
                embcraft_text::glyph(&data, f.index, '道')
                    .ok()
                    .flatten()
                    .filter(|g| !g.shapes.is_empty())
                    .map(|_| (data, f.index))
            })
    }

    #[test]
    fn convert_place_edit_and_save_a_font() {
        let Some((data, index)) = cjk_font() else {
            eprintln!("no CJK font installed; skipped");
            return;
        };
        let metrics = embcraft_text::font_metrics(&data, index).unwrap();
        let mut f = EcFont::new("Test Gothic", metrics, None);
        let r = f.convert(&data, index, "道カ태道\u{E000}", false).unwrap();
        assert_eq!(r.added, vec!['道', 'カ', '태']);
        assert_eq!(r.missing, vec!['\u{E000}']);
        // every converted shape carries its satin regions
        for g in f.glyphs.values() {
            assert!(!g.objects.is_empty());
            assert!(g.objects.iter().all(|o| o.satin_regions().is_some()));
        }
        // lettering: 20 mm characters, regions kept through scaling, stitchable
        let opts = TextOptions {
            cap_height: 20.0,
            size_is_em: true,
            ..TextOptions::default()
        };
        let (objs, missing) = f.layout("道カ", Point::new(10.0, 40.0), &opts);
        assert!(missing.is_empty());
        assert!(objs.iter().all(|o| o.satin_regions().is_some()));
        let (lo, hi) = objs.iter().filter_map(|o| o.bounds()).fold(
            (
                Point::new(f64::MAX, f64::MAX),
                Point::new(f64::MIN, f64::MIN),
            ),
            |(a, b), (c, d)| {
                (
                    Point::new(a.x.min(c.x), a.y.min(c.y)),
                    Point::new(b.x.max(d.x), b.y.max(d.y)),
                )
            },
        );
        assert!(
            hi.y <= 40.0 + 20.0 * 0.15 && lo.y >= 40.0 - 20.0,
            "{lo:?} {hi:?}"
        );
        assert!(hi.x - lo.x > 30.0, "two characters side by side");
        // edit a character, convert again: the edit survives
        let mut edited = f.glyph_for_editing('カ');
        edited.truncate(1);
        f.store_edited('カ', &edited, 1.0);
        let r = f.convert(&data, index, "カ", true).unwrap();
        assert_eq!(r.kept, vec!['カ']);
        assert_eq!(f.glyphs[&'カ'].objects.len(), 1);
        // file round trip
        let dir = std::env::temp_dir().join(format!("ecfont-test-{}", std::process::id()));
        let path = dir.join("t.ecfont");
        f.save(&path).unwrap();
        let back = EcFont::load(&path).unwrap();
        assert_eq!(back, f);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(EcFont::from_json("{}").is_err());
    }

    #[test]
    fn character_sets() {
        assert!(katakana().contains('カ') && katakana().contains('ー'));
        assert!(hiragana().contains('か'));
        assert!(martial_arts().contains('道') && martial_arts().contains('태'));
    }
}
