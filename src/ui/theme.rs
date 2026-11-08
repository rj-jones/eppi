//! The look, taken from blabbur's default theme (Melange) by way of
//! dsh-sessions: warm brown-black surfaces, cream text, tan secondary text
//! and a burnt orange accent, with blabbur's type scale, spacing and
//! rounding. Ionicons (blabbur's icon font) draws the symbols.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText,
};

pub const STRIP: Color32 = hex(0x1C1814);
pub const SIDEBAR: Color32 = hex(0x241F1A);
pub const SURFACE: Color32 = hex(0x2A2520);
pub const INPUT: Color32 = hex(0x1C1814);
pub const TEXT: Color32 = hex(0xECE1D7);
pub const MUTED: Color32 = hex(0xB09B86);
pub const FAINT: Color32 = hex(0x7D6D5E);
pub const SELECTED_TEXT: Color32 = hex(0xF3E9DF);
pub const WARNING: Color32 = hex(0xEBC06D);
pub const DANGER: Color32 = hex(0xC47778);
pub const SUCCESS: Color32 = hex(0x85B695);
pub const ACCENT: Color32 = hex(0xA95F33);
pub const ACCENT_TEXT: Color32 = hex(0xE49B5D);
pub const SELECTED_ROW: Color32 = hex(0x433E39);
pub const HOVERED_ROW: Color32 = hex(0x342F2A);
/// Behind a banner: blabbur's mention background.
pub const NOTICE: Color32 = hex(0x3E3425);
pub const BUTTON: Color32 = hex(0x3A342E);
pub const BUTTON_HOVERED: Color32 = hex(0x474039);
pub const BUTTON_PRESSED: Color32 = hex(0x534B43);
pub const SEPARATOR: Color32 = hex(0x39342F);
pub const OUTLINE_ACTIVE: Color32 = hex(0xE49B5D);

/// A won game or set.
pub const WIN: Color32 = SUCCESS;
/// A lost game or set.
pub const LOSS: Color32 = DANGER;

pub const fn hex(value: u32) -> Color32 {
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

/// Type scale in points (blabbur's).
pub mod text {
    pub const TINY: f32 = 11.0;
    pub const SMALL: f32 = 12.0;
    pub const BODY: f32 = 14.0;
    pub const LIST: f32 = 15.0;
    pub const TITLE: f32 = 16.0;
    pub const HEADING: f32 = 18.0;
    pub const DISPLAY: f32 = 26.0;
    pub const MONO: f32 = 13.0;
}

/// Spacing scale in points (blabbur's).
pub mod space {
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
}

/// The family for headings and names: Ubuntu Medium, a heavier cut of
/// egui's text font, as blabbur uses for names.
pub const HEAVY: &str = "ubuntu-medium";
/// The icon font's family.
pub const ICONS: &str = "ionicons";

static HEAVY_FONT: &[u8] = include_bytes!("../../assets/fonts/Ubuntu-Medium.ttf");
static ICON_FONT: &[u8] = include_bytes!("../../assets/fonts/Ionicons.ttf");

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert(HEAVY.to_owned(), FontData::from_static(HEAVY_FONT).into());
    fonts
        .font_data
        .insert(ICONS.to_owned(), FontData::from_static(ICON_FONT).into());
    let text_fonts = fonts.families[&FontFamily::Proportional].clone();
    let mut heavy = vec![HEAVY.to_owned()];
    heavy.extend(text_fonts.iter().cloned());
    fonts.families.insert(FontFamily::Name(HEAVY.into()), heavy);
    // A text font after the icons, for the replacement glyph egui looks
    // for in every family (Ionicons has none).
    let mut icons = vec![ICONS.to_owned()];
    icons.extend(text_fonts.first().cloned());
    fonts.families.insert(FontFamily::Name(ICONS.into()), icons);
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals());
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        use egui::TextStyle;
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(text::SMALL)),
            (TextStyle::Body, FontId::proportional(text::BODY)),
            (TextStyle::Button, FontId::proportional(text::BODY)),
            (
                TextStyle::Heading,
                FontId::new(text::HEADING, FontFamily::Name(HEAVY.into())),
            ),
            (TextStyle::Monospace, FontId::monospace(text::MONO)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(space::S, 6.0);
        style.spacing.button_padding = egui::vec2(space::M, 5.0);
        style.spacing.interact_size.y = 28.0;
        style.spacing.window_margin = Margin::same(space::L as i8);
        style.spacing.menu_margin = Margin::same(space::S as i8);
        let mut scroll = egui::style::ScrollStyle::floating();
        scroll.floating_allocated_width = 14.0;
        style.spacing.scroll = scroll;
        style.interaction.selectable_labels = false;
        style.interaction.tooltip_delay = 0.35;
    });
}

/// blabbur's `visuals_of` for the Melange palette.
fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = SURFACE;
    visuals.window_fill = SIDEBAR;
    visuals.window_stroke = egui::Stroke::new(1.0, SEPARATOR);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.error_fg_color = DANGER;
    visuals.warn_fg_color = WARNING;
    visuals.extreme_bg_color = INPUT;
    visuals.faint_bg_color = SIDEBAR;
    visuals.code_bg_color = SEPARATOR;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = egui::Stroke::new(1.0, SELECTED_TEXT);
    visuals.hyperlink_color = ACCENT_TEXT;
    let widgets = &mut visuals.widgets;
    widgets.noninteractive.fg_stroke.color = TEXT;
    widgets.inactive.fg_stroke.color = TEXT;
    widgets.hovered.fg_stroke.color = SELECTED_TEXT;
    widgets.active.fg_stroke.color = SELECTED_TEXT;
    widgets.noninteractive.bg_stroke.color = SEPARATOR;
    widgets.noninteractive.bg_fill = SIDEBAR;
    widgets.inactive.weak_bg_fill = BUTTON;
    widgets.inactive.bg_fill = BUTTON;
    widgets.inactive.bg_stroke = egui::Stroke::NONE;
    widgets.hovered.weak_bg_fill = BUTTON_HOVERED;
    widgets.hovered.bg_fill = BUTTON_HOVERED;
    widgets.hovered.bg_stroke = egui::Stroke::new(1.0, OUTLINE_ACTIVE);
    widgets.active.weak_bg_fill = BUTTON_PRESSED;
    widgets.active.bg_fill = BUTTON_PRESSED;
    widgets.active.bg_stroke = egui::Stroke::new(1.0, OUTLINE_ACTIVE);
    widgets.open.weak_bg_fill = BUTTON;
    widgets.open.bg_fill = SIDEBAR;
    widgets.open.bg_stroke = egui::Stroke::new(1.0, SEPARATOR);
    widgets.noninteractive.weak_bg_fill = BUTTON;
    for widget in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        widget.corner_radius = CornerRadius::same(5);
    }
    visuals.window_shadow.color = Color32::from_black_alpha(110);
    visuals.popup_shadow.color = Color32::from_black_alpha(100);
    visuals
}

/// Secondary text in the muted colour (egui's `weak()` is too faint on
/// these surfaces).
pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(MUTED)
}

/// An uppercase caption above a list section or a field.
pub fn caption(text: &str) -> RichText {
    muted(text.to_uppercase()).size(text::SMALL)
}

pub fn heavy(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, FontFamily::Name(HEAVY.into())))
}

pub fn heavy_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(HEAVY.into()))
}

/// An accent button with an icon in front of its label.
pub fn primary_icon_button(icon: Ion, label: &str) -> egui::Button<'static> {
    egui::Button::new(icon_label(icon, label, Color32::WHITE)).fill(ACCENT)
}

/// A plain button with an icon in front of its label.
pub fn icon_button(icon: Ion, label: &str) -> egui::Button<'static> {
    egui::Button::new(icon_label(icon, label, TEXT))
}

/// A small rounded label ("Ranked", "2–1").
pub fn pill(ui: &mut egui::Ui, label: impl Into<String>, color: Color32) -> egui::Response {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.20))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(label.into()).color(color).size(text::SMALL));
        })
        .response
}

/// A section card: a rounded sidebar-coloured box.
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(SIDEBAR)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(space::M as i8))
}

/// One Ionicons symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ion(pub char);

/// The symbols, by their Ionicons names (a catalogue: not all are used).
#[allow(dead_code)]
impl Ion {
    pub const MOON: Ion = Ion('\u{f460}');
    pub const LIST: Ion = Ion('\u{f3bb}');
    pub const PEOPLE: Ion = Ion('\u{f49f}');
    pub const PERSON: Ion = Ion('\u{f4a5}');
    pub const PERSON_CIRCLE: Ion = Ion('\u{f4a9}');
    pub const FLASH: Ion = Ion('\u{f316}');
    pub const TROPHY: Ion = Ion('\u{f601}');
    pub const FOLDER_OPEN: Ion = Ion('\u{f326}');
    pub const FOLDER: Ion = Ion('\u{f325}');
    pub const FILM: Ion = Ion('\u{f2fe}');
    pub const TRENDING_UP: Ion = Ion('\u{f5fb}');
    pub const PODIUM: Ion = Ion('\u{f4e4}');
    pub const SKULL: Ion = Ion('\u{f586}');
    pub const FLAME: Ion = Ion('\u{f313}');
    pub const GAME_CONTROLLER: Ion = Ion('\u{f334}');
    pub const PULSE: Ion = Ion('\u{f4f6}');
    pub const ANALYTICS: Ion = Ion('\u{f11b}');
    pub const FUNNEL: Ion = Ion('\u{f331}');
    pub const LAYERS: Ion = Ion('\u{f3af}');
    pub const ALBUMS: Ion = Ion('\u{f10f}');
    pub const BOOKMARK: Ion = Ion('\u{f1a8}');
    pub const ALERT_CIRCLE: Ion = Ion('\u{f113}');
    pub const SYNC: Ion = Ion('\u{f5b6}');
    pub const HOURGLASS: Ion = Ion('\u{f385}');
    pub const RIBBON: Ion = Ion('\u{f544}');
    pub const MEDAL: Ion = Ion('\u{f445}');
    pub const ARROW_UP: Ion = Ion('\u{f145}');
    pub const ARROW_DOWN: Ion = Ion('\u{f12d}');
    pub const CARET_UP: Ion = Ion('\u{f1fc}');
    pub const CARET_DOWN: Ion = Ion('\u{f1f0}');
    pub const CARET_FORWARD: Ion = Ion('\u{f1f6}');
    pub const SEARCH: Ion = Ion('\u{f55f}');
    pub const CLOSE: Ion = Ion('\u{f24a}');
    pub const CLOSE_CIRCLE: Ion = Ion('\u{f24b}');
    pub const DOCUMENT_TEXT: Ion = Ion('\u{f2b3}');
    pub const CREATE: Ion = Ion('\u{f292}');
    pub const EYE: Ion = Ion('\u{f2e6}');
    pub const TIME: Ion = Ion('\u{f5dd}');
    pub const CALENDAR: Ion = Ion('\u{f1cf}');
    pub const STATS_CHART: Ion = Ion('\u{f59b}');
    pub const REFRESH: Ion = Ion('\u{f514}');
    pub const OPEN: Ion = Ion('\u{f487}');
    pub const PLAY: Ion = Ion('\u{f4c6}');
    pub const PLAY_FORWARD: Ion = Ion('\u{f4d0}');
    pub const SETTINGS: Ion = Ion('\u{f56b}');
    pub const STAR: Ion = Ion('\u{f595}');
    pub const TRASH: Ion = Ion('\u{f5f2}');
    pub const COPY: Ion = Ion('\u{f28f}');
    pub const CHEVRON_FORWARD: Ion = Ion('\u{f23b}');
    pub const CHEVRON_DOWN: Ion = Ion('\u{f232}');
    pub const CHEVRON_BACK: Ion = Ion('\u{f229}');
    pub const INFORMATION_CIRCLE: Ion = Ion('\u{f398}');
    pub const WARNING: Ion = Ion('\u{f628}');
    pub const EARTH: Ion = Ion('\u{f2c2}');
    pub const SPARKLES: Ion = Ion('\u{f58c}');
    pub const PENCIL: Ion = Ion('\u{f49c}');
    pub const ADD: Ion = Ion('\u{f103}');
    pub const CHECKMARK: Ion = Ion('\u{f21d}');
    pub const ARROW_BACK: Ion = Ion('\u{f127}');

    pub fn rich(self, size: f32) -> RichText {
        RichText::new(self.0.to_string())
            .family(FontFamily::Name(ICONS.into()))
            .size(size)
    }

    pub fn font(size: f32) -> FontId {
        FontId::new(size, FontFamily::Name(ICONS.into()))
    }

    /// Paints the icon centred in `rect`.
    pub fn paint(self, painter: &egui::Painter, rect: egui::Rect, size: f32, color: Color32) {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            self.0,
            Self::font(size),
            color,
        );
    }
}

/// An icon followed by a label, as one line of text.
pub fn icon_label(icon: Ion, label: &str, color: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &icon.0.to_string(),
        0.0,
        egui::TextFormat {
            font_id: Ion::font(text::BODY + 1.0),
            color,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.append(
        label,
        6.0,
        egui::TextFormat {
            font_id: FontId::proportional(text::BODY),
            color,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job
}
