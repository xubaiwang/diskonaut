use ::std::path::PathBuf;
use ::tui::buffer::Buffer;
use ::tui::layout::Rect;
use ::tui::style::{Color, Modifier, Style};
use ::tui::widgets::Widget;

use crate::state::tiles::{FileType, Tile};
use crate::ui::format::{DisplaySize, truncate_middle};

fn render_currently_selected(buf: &mut Buffer, currently_selected: &Tile, max_len: u16, y: u16) {
    let file_name = currently_selected.name.to_string_lossy();
    let size = DisplaySize(currently_selected.size as f64);
    let descendants = currently_selected.descendants;
    let (style, lines) = match currently_selected.file_type {
        FileType::File => (
            Style::default().add_modifier(Modifier::BOLD),
            vec![
                format!("SELECTED: {} ({})", file_name, size),
                format!("SELECTED: {}", file_name),
                format!("{}", file_name),
            ],
        ),
        FileType::Folder => (
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            vec![
                format!(
                    "SELECTED: {} ({}, {} files)",
                    file_name,
                    size,
                    descendants.expect("a folder should have descendants")
                ),
                format!("SELECTED: {} ({})", file_name, size),
                format!("SELECTED: {}", file_name),
                format!("{}", file_name),
            ],
        ),
    };
    for line in lines {
        if (line.chars().count() as u16) < max_len {
            buf.set_string(1, y, line, style);
            break;
        }
    }
}

fn render_last_read_path(buf: &mut Buffer, last_read_path: &PathBuf, max_len: u16, y: u16) {
    let last_read_path = last_read_path.to_string_lossy();
    if (last_read_path.chars().count() as u16) < max_len {
        buf.set_string(1, y, last_read_path, Style::default());
    } else {
        buf.set_string(
            1,
            y,
            truncate_middle(&last_read_path, max_len),
            Style::default(),
        );
    }
}

fn render_controls_legend(buf: &mut Buffer, hide_delete: bool, max_len: u16, y: u16) {
    let (long_controls_line, short_controls_line) = if hide_delete {
        (
            String::from(
                "<arrows> - move around, <ENTER> - enter folder, <ESC> - parent folder, <+/-/0> - zoom in/out/reset, <q> - quit",
            ),
            String::from("←↓↑→/<ENTER>/<ESC>: navigate"),
        )
    } else {
        (
            String::from(
                "<arrows> - move around, <ENTER> - enter folder, <ESC> - parent folder, <BACKSPACE> - delete, <+/-/0> - zoom in/out/reset, <q> - quit",
            ),
            String::from("←↓↑→/<ENTER>/<ESC>: navigate, <BACKSPACE>: del"),
        )
    };
    let too_small_line = "(...)";
    if max_len >= long_controls_line.chars().count() as u16 {
        buf.set_string(
            1,
            y,
            long_controls_line,
            Style::default().add_modifier(Modifier::BOLD),
        );
    } else if max_len >= short_controls_line.chars().count() as u16 {
        buf.set_string(
            1,
            y,
            short_controls_line,
            Style::default().add_modifier(Modifier::BOLD),
        );
    } else {
        buf.set_string(
            1,
            y,
            too_small_line,
            Style::default().add_modifier(Modifier::BOLD),
        );
    }
}

fn render_small_files_legend(buf: &mut Buffer, x: u16, y: u16, small_files_legend: &str) {
    buf.set_string(
        x,
        y,
        small_files_legend,
        Style::default()
            .fg(Color::Reset)
            .bg(Color::Reset)
            .remove_modifier(Modifier::all()),
    );
    let small_files_legend_character = buf.get_mut(x + 1, y);
    small_files_legend_character.set_style(Style::default().bg(Color::White).fg(Color::Black));
}

pub struct BottomLine<'a> {
    hide_delete: bool,
    hide_small_files_legend: bool,
    currently_selected: Option<&'a Tile>,
    last_read_path: Option<&'a PathBuf>,
}

impl<'a> BottomLine<'a> {
    pub fn new() -> Self {
        Self {
            hide_delete: false,
            hide_small_files_legend: false,
            currently_selected: None,
            last_read_path: None,
        }
    }
    pub fn hide_delete(mut self) -> Self {
        self.hide_delete = true;
        self
    }
    pub fn hide_small_files_legend(mut self, should_hide_small_files_legend: bool) -> Self {
        self.hide_small_files_legend = should_hide_small_files_legend;
        self
    }
    pub fn currently_selected(mut self, currently_selected: Option<&'a Tile>) -> Self {
        self.currently_selected = currently_selected;
        self
    }
    pub fn last_read_path(mut self, last_read_path: Option<&'a PathBuf>) -> Self {
        self.last_read_path = last_read_path;
        self
    }
}

impl<'a> Widget for BottomLine<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let small_files_legend = "(x = Small files)";
        let small_files_len = if self.hide_small_files_legend {
            0
        } else {
            small_files_legend.chars().count() as u16
        };
        let max_status_len = area.width - small_files_len - 1;
        let max_controls_len = area.width - 1;
        let status_line_y = area.y + area.height - 2;
        let controls_line_y = status_line_y + 1;
        if let Some(currently_selected) = self.currently_selected {
            render_currently_selected(buf, currently_selected, max_status_len, status_line_y);
        } else if let Some(last_read_path) = self.last_read_path {
            render_last_read_path(buf, last_read_path, max_status_len, status_line_y);
        }

        if !self.hide_small_files_legend {
            render_small_files_legend(
                buf,
                area.width - small_files_len - 1,
                status_line_y,
                small_files_legend,
            );
        }

        render_controls_legend(buf, self.hide_delete, max_controls_len, controls_line_y);
    }
}
