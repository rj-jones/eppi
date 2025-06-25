use egui::{self, Ui};

/// Number of manual rows used in the original table demo.
pub const NUM_MANUAL_ROWS: usize = 20;

/// Adds a thin horizontal separator as expanding content in the demo table.
pub fn expanding_content(ui: &mut Ui) {
    ui.add(egui::Separator::default().horizontal());
}

/// Returns a generic long text for demonstration purposes.
pub fn long_text(row_index: usize) -> String {
    format!("Row {row_index} has some long text that you may want to clip, or it will take up too much horizontal space!")
}

/// Returns true if the row is considered "thick" (i.e. taller).
pub fn thick_row(row_index: usize) -> bool {
    row_index % 6 == 0
}

/// Format a SystemTime into a relative representation (e.g. "3 days ago").
pub fn format_date(date: std::time::SystemTime) -> String {
    // For now, let's just show how many days ago the file was modified
    if let Ok(duration_since) = std::time::SystemTime::now().duration_since(date) {
        let days_ago = duration_since.as_secs() / 86_400;
        if days_ago == 0 {
            "Today".to_string()
        } else if days_ago == 1 {
            "1 day ago".to_string()
        } else if days_ago < 7 {
            format!("{days_ago} days ago")
        } else if days_ago < 30 {
            let weeks = days_ago / 7;
            if weeks == 1 {
                "1 week ago".to_string()
            } else {
                format!("{weeks} weeks ago")
            }
        } else {
            let months = days_ago / 30;
            if months == 1 {
                "1 month ago".to_string()
            } else {
                format!("{months} months ago")
            }
        }
    } else {
        "Unknown".to_string()
    }
}

/// Format a number of frames (at 60 fps) into mm:ss.
pub fn format_duration(frames: i32) -> String {
    let total_seconds = frames / 60; // Melee runs at 60 FPS
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;

    if minutes > 0 {
        format!("{minutes}:{seconds:02}")
    } else {
        format!("0:{seconds:02}")
    }
}
