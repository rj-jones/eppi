//! The filters under the header, shared by Replays, Opponents, Me and
//! Highlights.

use crate::app::Data;
use crate::melee;
use crate::model::{Filter, Mode, Period, RankTier, Season, SeasonFilter};
use crate::ui::theme;
use crate::ui::widgets;
use eframe::egui::{self, Margin, vec2};

pub fn show(ui: &mut egui::Ui, filter: &mut Filter, data: &Data) {
    egui::Frame::new()
        .fill(theme::SIDEBAR)
        .inner_margin(Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 8.0);
                when_picker(ui, "filter-when", filter, &data.seasons);
                ui.separator();
                for mode in Mode::ALL {
                    let on = filter.modes.contains(&mode);
                    let text = egui::RichText::new(mode.label()).color(if on {
                        theme::SELECTED_TEXT
                    } else {
                        theme::MUTED
                    });
                    if ui
                        .add(egui::Button::new(text).selected(on))
                        .on_hover_text("Show only these modes (none picked shows all)")
                        .clicked()
                    {
                        if on {
                            filter.modes.retain(|m| *m != mode);
                        } else {
                            filter.modes.push(mode);
                        }
                    }
                }
                ui.separator();
                character_picker(ui, "filter-mine", "My character", &mut filter.my_character);
                character_picker(
                    ui,
                    "filter-theirs",
                    "Their character",
                    &mut filter.their_character,
                );
                stage_picker(ui, &mut filter.stage);
                ui.separator();
                ui.end_row();
                rank_chips(ui, filter, data);
                ui.separator();
                ui.checkbox(&mut filter.hide_short, "Hide short games")
                    .on_hover_text("Games under 30 seconds: disconnects, handwarmers, ragequits");
                ui.checkbox(&mut filter.hide_cpu, "Hide CPU games");
                ui.checkbox(&mut filter.only_mine, "Only my games")
                    .on_hover_text("Games one of your connect codes played");
                if filter.active_count() > 0 && ui.button("Reset").clicked() {
                    let search = std::mem::take(&mut filter.search);
                    *filter = Filter {
                        search,
                        ..Filter::default()
                    };
                }
            });
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
