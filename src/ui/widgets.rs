//! Small pieces every view draws: stock icons, rank badges, result marks.

use crate::melee;
use crate::model::Outcome;
use crate::ui::theme::{self, Ion};
use eframe::egui::{self, Color32, CornerRadius, Rect, Sense, Vec2, vec2};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/assets.rs"));
}

/// An image source for the stock icon of `character` in `costume`.
pub fn stock_source(character: u8, costume: u8) -> Option<egui::ImageSource<'static>> {
    let (uri, bytes) =
        embedded::stock(character, costume).or_else(|| embedded::stock(character, 0))?;
    Some(egui::ImageSource::Bytes {
        uri: uri.into(),
        bytes: bytes.into(),
    })
}

pub fn stage_source(stage: u16) -> Option<egui::ImageSource<'static>> {
    let (uri, bytes) = embedded::stage(stage).or_else(|| embedded::stage(u16::MAX))?;
    Some(egui::ImageSource::Bytes {
        uri: uri.into(),
        bytes: bytes.into(),
    })
}

/// The rank icon for a rank name ("Diamond 2", "Grandmaster", "Pending").
pub fn rank_source(rank: &str) -> Option<egui::ImageSource<'static>> {
    let file = rank_icon_file(rank);
    let (uri, bytes) = embedded::rank_icon(&file)?;
    Some(egui::ImageSource::Bytes {
        uri: uri.into(),
        bytes: bytes.into(),
    })
}

/// "Diamond 2" → "DIAMOND 2" (the icon files' names); anything without a
/// tier icon gets the unranked one.
fn rank_icon_file(rank: &str) -> String {
    let upper = rank.to_uppercase();
    let upper = upper.replace("PLAT ", "PLATINUM ");
    match upper.as_str() {
        "GRANDMASTER" | "GRAND MASTER" => "GRANDMASTER".to_owned(),
        _ if ["BRONZE", "SILVER", "GOLD", "PLATINUM", "DIAMOND", "MASTER"]
            .iter()
            .any(|tier| upper.starts_with(tier)) =>
        {
            upper
        }
        _ => "UNRANKED".to_owned(),
    }
}

/// The launcher's colour for a rank's name.
pub fn rank_color(rank: &str) -> Color32 {
    let lower = rank.to_lowercase();
    let rgb = if lower.starts_with("bronze") {
        0xE06A36
    } else if lower.starts_with("silver") {
        0xB5A5B7
    } else if lower.starts_with("gold") {
        0xF6A51E
    } else if lower.starts_with("plat") {
        0x91E8E0
    } else if lower.starts_with("diamond") {
        0x5DDFF4
    } else if lower.starts_with("master") {
        0xA494EE
    } else if lower.starts_with("grand") {
        0xFE4B4B
    } else {
        0xB09B86
    };
    theme::hex(rgb)
}

/// A stock icon `size` points square; a grey box when the art is missing.
pub fn stock_icon(ui: &mut egui::Ui, character: u8, costume: u8, size: f32) -> egui::Response {
    match stock_source(character, costume) {
        Some(source) => ui
            .add(egui::Image::new(source).fit_to_exact_size(Vec2::splat(size)))
            .on_hover_text(melee::character_name(character)),
        None => {
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), theme::BUTTON);
            response.on_hover_text(melee::character_name(character))
        }
    }
}

/// Paints a stock icon into `rect` (for rows drawn with the painter).
pub fn paint_stock(ui: &egui::Ui, rect: Rect, character: u8, costume: u8, tint: Color32) {
    if let Some(source) = stock_source(character, costume) {
        egui::Image::new(source).tint(tint).paint_at(ui, rect);
    }
}

pub fn paint_rank(ui: &egui::Ui, rect: Rect, rank: &str) {
    if let Some(source) = rank_source(rank) {
        egui::Image::new(source).paint_at(ui, rect);
    }
}

pub fn outcome_color(outcome: Outcome) -> Color32 {
    match outcome {
        Outcome::Win => theme::WIN,
        Outcome::Loss => theme::LOSS,
        Outcome::Unknown => theme::MUTED,
    }
}

pub fn outcome_letter(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Win => "W",
        Outcome::Loss => "L",
        Outcome::Unknown => "–",
    }
}

/// A square W/L mark, `size` points.
pub fn paint_outcome(painter: &egui::Painter, rect: Rect, outcome: Outcome) {
    let color = outcome_color(outcome);
    painter.rect_filled(rect, CornerRadius::same(4), color.gamma_multiply(0.22));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        outcome_letter(outcome),
        theme::heavy_font(theme::text::SMALL),
        color,
    );
}

/// A row of stock icons for the stocks a player had left (greyed for lost
/// ones), `count` of `total`.
pub fn paint_stocks(
    ui: &egui::Ui,
    origin: egui::Pos2,
    character: u8,
    costume: u8,
    left: u8,
    total: u8,
    size: f32,
) -> f32 {
    let mut x = origin.x;
    for i in 0..total {
        let rect = Rect::from_min_size(egui::pos2(x, origin.y), Vec2::splat(size));
        let tint = if i < left {
            Color32::WHITE
        } else {
            Color32::from_white_alpha(40)
        };
        paint_stock(ui, rect, character, costume, tint);
        x += size + 2.0;
    }
    x - origin.x
}

/// A big number with a caption under it, for summary cards.
pub fn stat_tile(ui: &mut egui::Ui, value: impl Into<String>, caption: &str, color: Color32) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.label(theme::heavy(value, theme::text::DISPLAY).color(color));
        ui.label(theme::caption(caption));
    });
}

/// A clickable nav row: icon, label and an optional count.
pub fn nav_item(
    ui: &mut egui::Ui,
    icon: Ion,
    label: &str,
    count: Option<String>,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(6), theme::SELECTED_ROW);
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(6), theme::HOVERED_ROW);
    }
    let color = if selected {
        theme::SELECTED_TEXT
    } else {
        theme::MUTED
    };
    let icon_rect = Rect::from_min_size(rect.min + vec2(8.0, 0.0), vec2(22.0, rect.height()));
    icon.paint(
        painter,
        icon_rect,
        17.0,
        if selected { theme::ACCENT_TEXT } else { color },
    );
    painter.text(
        egui::pos2(rect.min.x + 38.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(theme::text::LIST),
        if selected {
            theme::SELECTED_TEXT
        } else {
            theme::TEXT
        },
    );
    if let Some(count) = count {
        painter.text(
            egui::pos2(rect.max.x - 10.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            count,
            egui::FontId::proportional(theme::text::SMALL),
            theme::FAINT,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Text clipped with "…" to fit `width`, painted left-aligned at `pos`
/// (vertically centred).
pub fn paint_text_clipped(
    painter: &egui::Painter,
    pos: egui::Pos2,
    text: &str,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> f32 {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(10.0));
    let galley = painter.layout_job(job);
    let size = galley.size();
    painter.galley(pos - vec2(0.0, size.y / 2.0), galley, color);
    size.x
}

/// "1,234".
pub fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// "1 game", "1,234 games".
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{} {noun}s", thousands(n))
    }
}

/// "62%" or "–".
pub fn percent(part: u32, whole: u32) -> String {
    if whole == 0 {
        "–".to_owned()
    } else {
        format!("{:.0}%", 100.0 * part as f64 / whole as f64)
    }
}

pub fn win_rate_color(wins: u32, total: u32) -> Color32 {
    if total == 0 {
        return theme::MUTED;
    }
    let rate = wins as f32 / total as f32;
    if rate >= 0.55 {
        theme::WIN
    } else if rate <= 0.45 {
        theme::LOSS
    } else {
        theme::TEXT
    }
}
