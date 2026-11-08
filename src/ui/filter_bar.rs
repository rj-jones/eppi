//! The filters, shared by Replays, Opponents, Me and Highlights. A row of
//! dropdowns sits over the list; under it, only while something isn't at
//! its default, a chip per filter applied (each with its own clear); and
//! the rest (modes, opponents' ranks, which games to hide) is in the menu
//! the Filters button by the search bar opens, to keep the list tall.

use crate::app::Data;
use crate::melee;
use crate::model::{Filter, Mode, Period, RankTier, Season, SeasonFilter};
use crate::ui::theme::{self, Ion};
use crate::ui::widgets;
use eframe::egui::{self, Margin, vec2};

/// The dropdowns, and the filters applied.
pub fn show(ui: &mut egui::Ui, filter: &mut Filter, data: &Data) {
    egui::Frame::new()
        .fill(theme::SIDEBAR)
        .inner_margin(Margin::symmetric(16, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 8.0);
                when_picker(ui, "filter-when", filter, &data.seasons);
                character_picker(ui, "filter-mine", "My character", &mut filter.my_character);
                character_picker(
                    ui,
                    "filter-theirs",
                    "Their character",
                    &mut filter.their_character,
                );
                stage_picker(ui, &mut filter.stage);
            });
            if filter.active_count() > 0 {
                ui.add_space(6.0);
                applied(ui, filter);
            }
        });
}

/// What the Filters button's menu holds: the modes, opponents' ranks and
/// which games to hide.
pub fn menu(ui: &mut egui::Ui, filter: &mut Filter, data: &Data) {
    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
    ui.label(theme::caption("Mode"));
    ui.horizontal_wrapped(|ui| {
        for mode in Mode::ALL {
            let on = filter.modes.contains(&mode);
            let text = egui::RichText::new(mode.label()).color(if on {
                egui::Color32::WHITE
            } else {
                theme::MUTED
            });
            if ui
                .add(egui::Button::new(text).selected(on))
                .on_hover_text("Only these modes (none picked shows them all)")
                .clicked()
            {
                if on {
                    filter.modes.retain(|m| *m != mode);
                } else {
                    filter.modes.push(mode);
                }
            }
        }
    });
    ui.add_space(4.0);
    ui.label(theme::caption("Opponent's rank"));
    ui.horizontal_wrapped(|ui| rank_chips(ui, filter, data));
    ui.add_space(4.0);
    ui.label(theme::caption("Games"));
    ui.checkbox(&mut filter.hide_short, "Hide short games")
        .on_hover_text("Games under 30 seconds: disconnects, handwarmers, ragequits");
    ui.checkbox(&mut filter.hide_cpu, "Hide CPU games");
    ui.checkbox(&mut filter.only_mine, "Only my games")
        .on_hover_text("Games one of your connect codes played");
    if filter.active_count() > 0 {
        ui.add_space(4.0);
        if ui.button("Clear all filters").clicked() {
            clear(filter);
        }
    }
}

/// Every filter back to its default (the search stays).
fn clear(filter: &mut Filter) {
    let search = std::mem::take(&mut filter.search);
    *filter = Filter {
        search,
        ..Filter::default()
    };
}

/// A chip for an applied filter: its icon if any, its words, and an × that
/// clears it. Returns whether it was clicked.
fn chip(ui: &mut egui::Ui, icon: Option<egui::ImageSource<'static>>, text: &str) -> bool {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(theme::text::SMALL + 1.0),
            color: theme::TEXT,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.append(
        &Ion::CLOSE.0.to_string(),
        6.0,
        egui::TextFormat {
            font_id: Ion::font(theme::text::SMALL + 1.0),
            color: theme::MUTED,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    let button = match icon {
        Some(icon) => egui::Button::image_and_text(
            egui::Image::new(icon).fit_to_exact_size(vec2(16.0, 16.0)),
            job,
        ),
        None => egui::Button::new(job),
    };
    ui.add(button.fill(theme::NOTICE).corner_radius(12))
        .on_hover_text("Remove this filter")
        .clicked()
}

/// The applied row: a chip for each filter not at its default, then Clear
/// all.
fn applied(ui: &mut egui::Ui, filter: &mut Filter) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        if let Some(season) = filter.season.clone() {
            if chip(ui, None, &season.name) {
                filter.season = None;
            }
        } else if filter.period != Period::All && chip(ui, None, filter.period.label()) {
            filter.period = Period::All;
        }
        for mode in filter.modes.clone() {
            if chip(ui, None, mode.label()) {
                filter.modes.retain(|m| *m != mode);
            }
        }
        if let Some(c) = filter.my_character {
            let text = format!("Me: {}", melee::character_name(c));
            if chip(ui, widgets::stock_source(c, 0), &text) {
                filter.my_character = None;
            }
        }
        if let Some(c) = filter.their_character {
            let text = format!("Them: {}", melee::character_name(c));
            if chip(ui, widgets::stock_source(c, 0), &text) {
                filter.their_character = None;
            }
        }
        if let Some(stage) = filter.stage
            && chip(ui, None, melee::stage_name(stage))
        {
            filter.stage = None;
        }
        for tier in filter.their_ranks.clone() {
            if chip(ui, widgets::rank_source(tier.icon_rank()), tier.label()) {
                filter.their_ranks.retain(|t| *t != tier);
            }
        }
        let defaults = Filter::default();
        if filter.hide_short != defaults.hide_short && chip(ui, None, "Short games shown") {
            filter.hide_short = defaults.hide_short;
        }
        if filter.hide_cpu != defaults.hide_cpu && chip(ui, None, "CPU games shown") {
            filter.hide_cpu = defaults.hide_cpu;
        }
        if filter.only_mine && chip(ui, None, "Only my games") {
            filter.only_mine = false;
        }
        if ui
            .add(egui::Button::new(theme::muted("Clear all").size(theme::text::SMALL)).frame(false))
            .clicked()
        {
            clear(filter);
        }
    });
}

fn character_picker(ui: &mut egui::Ui, id: &str, label: &str, value: &mut Option<u8>) {
    let text = value.map_or(label.to_owned(), |c| {
        format!("{label}: {}", melee::character_short(c))
    });
    egui::ComboBox::from_id_salt(id)
        .selected_text(text)
        .width(150.0)
        .height(420.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "Any");
            let mut ids = melee::PLAYABLE.to_vec();
            ids.sort_by_key(|c| melee::character_name(*c));
            for c in ids {
                ui.horizontal(|ui| {
                    widgets::stock_icon(ui, c, 0, 18.0);
                    ui.selectable_value(value, Some(c), melee::character_name(c));
                });
            }
        });
}

fn stage_picker(ui: &mut egui::Ui, value: &mut Option<u16>) {
    let text = value.map_or("Stage".to_owned(), |s| melee::stage_short(s).to_owned());
    egui::ComboBox::from_id_salt("filter-stage")
        .selected_text(text)
        .width(130.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "Any");
            for stage in melee::LEGAL_STAGES {
                ui.selectable_value(value, Some(stage), melee::stage_name(stage));
            }
        });
}

/// "When": a time period, or a ranked season (newest first). Picking one
/// clears the other.
pub fn when_picker(ui: &mut egui::Ui, id: &str, filter: &mut Filter, seasons: &[Season]) {
    let selected = match &filter.season {
        Some(season) => season.name.clone(),
        None => filter.period.label().to_owned(),
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .width(150.0)
        .height(460.0)
        .show_ui(ui, |ui| {
            for period in Period::ALL {
                let on = filter.season.is_none() && filter.period == period;
                if ui.selectable_label(on, period.label()).clicked() {
                    filter.period = period;
                    filter.season = None;
                }
            }
            if seasons.is_empty() {
                return;
            }
            ui.separator();
            ui.label(theme::caption("Ranked seasons"));
            for season in seasons {
                let on = filter.season.as_ref().is_some_and(|s| s.id == season.id);
                let dates = match season.end {
                    Some(end) => format!(
                        "{} – {}",
                        crate::time::month_label(season.start),
                        crate::time::month_label(end)
                    ),
                    None => format!("since {}", crate::time::day_label(season.start)),
                };
                if ui
                    .selectable_label(on, format!("{}  ·  {dates}", season.name))
                    .on_hover_text("Games played while this season ran (add the Ranked mode for only its ranked games)")
                    .clicked()
                {
                    filter.season = Some(SeasonFilter::from(season));
                    filter.period = Period::All;
                }
            }
        });
}

/// A chip per rank tier: games against opponents of the picked tiers.
fn rank_chips(ui: &mut egui::Ui, filter: &mut Filter, data: &Data) {
    const TIP: &str = "Only games against opponents of this rank, as they were in the season \
         you played them (how they finished it, or their rank now for this season). Pick \
         several to combine them. Games from before ranked (Dec 2022) don't match.";
    for tier in RankTier::ALL {
        let on = filter.their_ranks.contains(&tier);
        // On the orange of a picked chip, white reads; the icon carries
        // the tier's colour.
        let text = egui::RichText::new(tier.label()).color(if on {
            egui::Color32::WHITE
        } else {
            theme::MUTED
        });
        let button = match widgets::rank_source(tier.icon_rank()) {
            Some(icon) => egui::Button::image_and_text(
                egui::Image::new(icon).fit_to_exact_size(vec2(16.0, 16.0)),
                text,
            ),
            None => egui::Button::new(text),
        };
        if ui.add(button.selected(on)).on_hover_text(TIP).clicked() {
            if on {
                filter.their_ranks.retain(|t| *t != tier);
            } else {
                filter.their_ranks.push(tier);
            }
        }
    }
    if !filter.their_ranks.is_empty() && data.ranks_pending > 0 {
        ui.add(egui::Spinner::new().size(12.0));
        ui.label(
            theme::muted(format!(
                "Looking up {} players' seasons…",
                widgets::thousands(data.ranks_pending)
            ))
            .size(theme::text::SMALL),
        );
    }
}
