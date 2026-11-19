//! Notes Overlay
//!
//! Displays encounter notes in a text overlay.
//! Notes are written in Markdown format and displayed with basic formatting.

use super::{Overlay, OverlayConfigUpdate, OverlayData};
use crate::frame::OverlayFrame;
use crate::platform::{OverlayConfig, PlatformError};
use crate::utils::color_from_rgba;

/// Configuration for the notes overlay (matches baras_types::NotesOverlayConfig)
#[derive(Debug, Clone)]
pub struct NotesConfig {
    /// Font size for notes text
    pub font_size: u8,
    /// Font color for notes text (RGBA)
    pub font_color: [u8; 4],
    pub dynamic_background: bool,
}

impl Default for NotesConfig {
    fn default() -> Self {
        Self {
            font_size: 14,
            font_color: [255, 255, 255, 255],
            dynamic_background: false,
        }
    }
}

/// Data sent from service to notes overlay
#[derive(Debug, Clone, Default)]
pub struct NotesData {
    /// The notes text (Markdown format)
    pub text: String,
    /// Boss/encounter name for the header
    pub boss_name: String,
}

/// Base dimensions for scaling calculations
const BASE_WIDTH: f32 = 400.0;
const BASE_HEIGHT: f32 = 300.0;

/// Base layout values (at BASE_WIDTH x BASE_HEIGHT)
const BASE_LINE_HEIGHT: f32 = 18.0;
const BASE_PADDING: f32 = 8.0;
const BASE_HEADER_SPACING: f32 = 6.0;

/// Notes text overlay
pub struct NotesOverlay {
    frame: OverlayFrame,
    config: NotesConfig,
    /// Current notes data
    data: NotesData,
    /// Cached parsed lines for rendering
    lines: Vec<NotesLine>,
    european_number_format: bool,
}

/// A text span with styling
#[derive(Debug, Clone)]
struct TextSpan {
    text: String,
    bold: bool,
    italic: bool,
}

/// A processed line of notes text with styling
#[derive(Debug, Clone)]
struct NotesLine {
    /// The styled text spans to display
    spans: Vec<TextSpan>,
    /// Whether this is a header line
    is_header: bool,
    /// Whether this is a horizontal divider
    is_divider: bool,
    /// Indent level (for bullet points/numbered lists)
    _indent: u8,
}

impl NotesOverlay {
    /// Create a new notes overlay
    pub fn new(window_config: OverlayConfig, config: NotesConfig) -> Result<Self, PlatformError> {
        let mut frame = OverlayFrame::new(window_config, BASE_WIDTH, BASE_HEIGHT)?;
        frame.set_label("Notes");

        Ok(Self {
            frame,
            config,
            data: NotesData::default(),
            lines: Vec::new(),
            european_number_format: false,
        })
    }

    /// Update the config
    pub fn set_config(&mut self, config: NotesConfig) {
        self.config = config;
    }

    /// Set the notes data and parse it for rendering
    pub fn set_data(&mut self, data: NotesData) {
        self.data = data;
        self.parse_lines();
    }

    /// Clear the notes
    pub fn clear(&mut self) {
        self.data = NotesData::default();
        self.lines.clear();
    }

    /// Parse the markdown text into renderable lines
    fn parse_lines(&mut self) {
        self.lines.clear();

        for line in self.data.text.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() {
                // Keep empty lines for spacing
                self.lines.push(NotesLine {
                    spans: vec![],
                    is_header: false,
                    is_divider: false,
                    _indent: 0,
                });
                continue;
            }

            // Check for horizontal divider (---, ***, ___)
            if trimmed == "---" || trimmed == "***" || trimmed == "___" {
                self.lines.push(NotesLine {
                    spans: vec![],
                    is_header: false,
                    is_divider: true,
                    _indent: 0,
                });
                continue;
            }

            // Check for headers (## Header)
            if trimmed.starts_with("##") {
                let text = trimmed.trim_start_matches('#').trim().to_string();
                self.lines.push(NotesLine {
                    spans: vec![TextSpan {
                        text,
                        bold: true,
                        italic: false,
                    }],
                    is_header: true,
                    is_divider: false,
                    _indent: 0,
                });
                continue;
            }

            // Check for numbered lists (1. item, 2. item, etc.)
            let numbered_list_match = self.parse_numbered_list(trimmed);
            if let Some((num, content)) = numbered_list_match {
                let prefix = format!("  {}. ", num);
                let mut spans = vec![TextSpan {
                    text: prefix,
                    bold: false,
                    italic: false,
                }];
                spans.extend(self.parse_inline_spans(content));
                self.lines.push(NotesLine {
                    spans,
                    is_header: false,
                    is_divider: false,
                    _indent: 1,
                });
                continue;
            }

            // Check for bullet points (- item or * item, but not ** which is bold)
            if trimmed.starts_with("- ") || (trimmed.starts_with("* ") && !trimmed.starts_with("**")) {
                let content = trimmed[2..].trim();
                let prefix = format!("  {} ", '\u{2022}');
                let mut spans = vec![TextSpan {
                    text: prefix,
                    bold: false,
                    italic: false,
                }];
                spans.extend(self.parse_inline_spans(content));
                self.lines.push(NotesLine {
                    spans,
                    is_header: false,
                    is_divider: false,
                    _indent: 1,
                });
                continue;
            }

            // Check for nested bullet points
            if trimmed.starts_with("  - ") || trimmed.starts_with("  * ") {
                let content = trimmed[4..].trim();
                let prefix = format!("    {} ", '\u{2022}');
                let mut spans = vec![TextSpan {
                    text: prefix,
                    bold: false,
                    italic: false,
                }];
                spans.extend(self.parse_inline_spans(content));
                self.lines.push(NotesLine {
                    spans,
                    is_header: false,
                    is_divider: false,
                    _indent: 2,
                });
                continue;
            }

            // Regular text - parse inline spans
            let spans = self.parse_inline_spans(trimmed);
            self.lines.push(NotesLine {
                spans,
                is_header: false,
                is_divider: false,
                _indent: 0,
            });
        }
    }

    /// Parse inline formatting into styled spans
    fn parse_inline_spans(&self, text: &str) -> Vec<TextSpan> {
        let mut spans = Vec::new();
        let mut current_text = String::new();
        let mut in_bold = false;
        let mut in_italic = false;
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            // Check for ** (bold toggle)
            if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
                // Save current span if not empty
                if !current_text.is_empty() {
                    spans.push(TextSpan {
                        text: current_text.clone(),
                        bold: in_bold,
                        italic: in_italic,
                    });
                    current_text.clear();
                }
                in_bold = !in_bold;
                i += 2;
                continue;
            }

            // Check for single * (italic toggle) - but only if not part of **
            if chars[i] == '*' {
                // Save current span if not empty
                if !current_text.is_empty() {
                    spans.push(TextSpan {
                        text: current_text.clone(),
                        bold: in_bold,
                        italic: in_italic,
                    });
                    current_text.clear();
                }
                in_italic = !in_italic;
                i += 1;
                continue;
            }

            current_text.push(chars[i]);
            i += 1;
        }

        // Add remaining text
        if !current_text.is_empty() {
            spans.push(TextSpan {
                text: current_text,
                bold: in_bold,
                italic: in_italic,
            });
        }

        // If no spans were created, return a single empty span
        if spans.is_empty() {
            spans.push(TextSpan {
                text: String::new(),
                bold: false,
                italic: false,
            });
        }

        spans
    }

    /// Try to parse a numbered list item (e.g., "1. text")
    fn parse_numbered_list<'a>(&self, text: &'a str) -> Option<(&'a str, &'a str)> {
        // Find the first dot
        let dot_pos = text.find('.')?;

        // Check if everything before the dot is digits
        let num_part = &text[..dot_pos];
        if num_part.is_empty() || !num_part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }

        // Check if there's a space after the dot
        let rest = &text[dot_pos + 1..];
        if !rest.starts_with(' ') {
            return None;
        }

        Some((num_part, rest[1..].trim()))
    }

    /// Render the overlay
    pub fn render(&mut self) {
        let padding = self.frame.scaled(BASE_PADDING);
        let line_height = self.frame.scaled(BASE_LINE_HEIGHT);
        let header_spacing = self.frame.scaled(BASE_HEADER_SPACING);
        let font_size = self.frame.scaled(self.config.font_size as f32);
        let header_font_size = font_size * 1.1;

        // Calculate content height for dynamic background
        if self.config.dynamic_background {
            let mut content_h = padding;
            if !self.data.boss_name.is_empty() {
                content_h += header_font_size + header_spacing * 2.0;
            }
            for line in &self.lines {
                if line.is_divider {
                    content_h += font_size + line_height * 0.3;
                } else if line.spans.is_empty() || line.spans.iter().all(|s| s.text.is_empty()) {
                    content_h += line_height * 0.5;
                } else {
                    content_h += line_height;
                }
            }
            content_h += padding;
            self.frame.begin_frame_with_content_height(content_h);
        } else {
            self.frame.begin_frame();
        }

        // If no data, show placeholder
        if self.data.text.is_empty() && self.data.boss_name.is_empty() {
            self.frame.draw_text_glowed(
                "No notes",
                padding,
                padding + font_size,
                font_size,
                color_from_rgba([128, 128, 128, 128]),
            );
            self.frame.end_frame();
            return;
        }

        let mut y = padding;

        // Draw boss name header if present
        if !self.data.boss_name.is_empty() {
            y += header_font_size;
            self.frame.draw_text_glowed(
                &self.data.boss_name,
                padding,
                y,
                header_font_size,
                color_from_rgba([255, 220, 100, 255]), // Yellow/gold for header
            );
            y += header_spacing * 2.0;
        }

        // Draw notes lines
        let divider_color = color_from_rgba([100, 100, 100, 200]);

        for line in &self.lines {
            // Handle dividers - draw as a line of dashes
            if line.is_divider {
                y += font_size;
                // Check bounds
                if y > self.frame.height() as f32 - padding {
                    break;
                }
                // Draw divider as repeated dash characters
                self.frame.draw_text_glowed(
                    "────────────────────────────",
                    padding,
                    y,
                    font_size * 0.8,
                    divider_color,
                );
                y += line_height * 0.3;
                continue;
            }

            // Handle empty lines (spacing)
            if line.spans.is_empty() || line.spans.iter().all(|s| s.text.is_empty()) {
                y += line_height * 0.5;
                continue;
            }

            y += font_size;

            // Check if we've exceeded the visible area
            if y > self.frame.height() as f32 - padding {
                break;
            }

            let color = if line.is_header {
                // Section headers in a slightly different color (light blue)
                [200, 200, 255, 255]
            } else {
                self.config.font_color
            };

            let size = if line.is_header { font_size * 1.05 } else { font_size };

            // Render each span at incrementing x positions
            let mut x = padding;
            for span in &line.spans {
                if span.text.is_empty() {
                    continue;
                }
                // Headers are always bold
                let use_bold = span.bold || line.is_header;
                let use_italic = span.italic;

                self.frame
                    .draw_text_styled(&span.text, x, y, size, color_from_rgba(color), use_bold, use_italic);

                // Advance x position by the width of the rendered text (must match style used for drawing)
                let (w, _) = self.frame.measure_text_styled(&span.text, size, use_bold, use_italic);
                x += w;
            }

            y += line_height - font_size;
        }

        // End frame (resize indicator, commit)
        self.frame.end_frame();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overlay Trait Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl Overlay for NotesOverlay {
    fn update_data(&mut self, data: OverlayData) -> bool {
        if let OverlayData::Notes(notes_data) = data {
            let was_empty = self.data.text.is_empty();
            let is_empty = notes_data.text.is_empty();
            let changed = self.data.text != notes_data.text || self.data.boss_name != notes_data.boss_name;

            if changed {
                self.set_data(notes_data);
                true
            } else {
                // Return true if we went from empty to non-empty or vice versa
                was_empty != is_empty
            }
        } else {
            false
        }
    }

    fn update_config(&mut self, config: OverlayConfigUpdate) {
        if let OverlayConfigUpdate::Notes(notes_config, european) = config {
            self.set_config(notes_config);
            self.european_number_format = european;
        }
    }

    fn render(&mut self) {
        NotesOverlay::render(self);
    }

    fn poll_events(&mut self) -> bool {
        self.frame.poll_events()
    }

    fn frame(&self) -> &OverlayFrame {
        &self.frame
    }

    fn frame_mut(&mut self) -> &mut OverlayFrame {
        &mut self.frame
    }
}
