//! The filters under the header, shared by Replays, Opponents, Me and
//! Highlights.

use crate::app::Data;
use crate::melee;
use crate::model::{Filter, Mode, Period};
use crate::ui::theme;
use crate::ui::widgets;
use eframe::egui::{self, Margin, vec2};

pub fn show(ui: &mut egui::Ui, filter: &mut Filter, _data: &Data) {
    egui::Frame::new()
        .fill(theme::SIDEBAR)
        .inner_margin(Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 8.0);
                egui::ComboBox::from_id_salt("filter-period")
                    .selected_text(filter.period.label())
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for period in Period::ALL {
                            ui.selectable_value(&mut filter.period, period, period.label());
                        }
                    });
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
