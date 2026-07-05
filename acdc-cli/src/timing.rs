use std::{path::PathBuf, time::Duration};

use acdc_converters_core::PrettyDuration;

pub(crate) struct TimingEntry {
    pub(crate) path: PathBuf,
    pub(crate) parse: Duration,
    pub(crate) convert: Duration,
}

pub(crate) trait TimingRenderer {
    fn render(&self, wall_clock: Option<Duration>);
}

impl TimingRenderer for [TimingEntry] {
    fn render(&self, wall_clock: Option<Duration>) {
        if self.is_empty() {
            return;
        }

        let mut sorted: Vec<_> = self.iter().collect();
        sorted.sort_by_key(|e| e.parse + e.convert);

        let name_width = sorted
            .iter()
            .map(|t| t.path.file_name().map_or(0, |n| n.to_string_lossy().len()))
            .max()
            .unwrap_or(4)
            .max(4);

        let col = 12;
        let separator_len = name_width + 2 + col * 3 + 4;

        eprintln!(
            "\n{:<nw$}  {:>cw$}  {:>cw$}  {:>cw$}",
            "File",
            "Parse",
            "Convert",
            "Total",
            nw = name_width,
            cw = col
        );
        eprintln!("{}", "\u{2500}".repeat(separator_len));

        for entry in &sorted {
            let total = entry.parse + entry.convert;
            let name = entry.path.file_name().map_or_else(
                || entry.path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            eprintln!(
                "{:<nw$}  {:>cw$}  {:>cw$}  {:>cw$}",
                name,
                entry.parse.pretty_print(),
                entry.convert.pretty_print(),
                total.pretty_print(),
                nw = name_width,
                cw = col
            );
        }

        eprintln!("{}", "\u{2500}".repeat(separator_len));
        let total_parse: Duration = self.iter().map(|t| t.parse).sum();
        let total_convert: Duration = self.iter().map(|t| t.convert).sum();
        let total_all = total_parse + total_convert;
        let label = format!("Total ({} files)", self.len());
        eprintln!(
            "{:<nw$}  {:>cw$}  {:>cw$}  {:>cw$}",
            label,
            total_parse.pretty_print(),
            total_convert.pretty_print(),
            total_all.pretty_print(),
            nw = name_width,
            cw = col
        );

        if let Some(wall) = wall_clock {
            eprintln!(
                "{:<nw$}  {:>cw$}",
                "Wall clock",
                wall.pretty_print(),
                nw = name_width + 2 + col * 2 + 2,
                cw = col
            );
        }
    }
}
