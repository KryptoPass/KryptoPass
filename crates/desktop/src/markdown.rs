//! Renders note Markdown as Qt's restricted rich text.
//! Original Markdown remains separate from the disposable rendered document.

use comrak::{
    Arena, Options, create_formatter, html::ChildRendering, nodes::NodeValue, parse_document,
};
use std::{
    collections::{BTreeSet, HashMap},
    fmt::{self, Write},
    sync::atomic::{AtomicU64, Ordering},
};
use zeroize::Zeroizing;

pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
const ACTION_SCHEME: &str = "kryptopass-spoiler:";
static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct Theme {
    pub highlight_background: String,
    pub highlight_foreground: String,
    pub link: String,
    pub code_background: String,
    pub border: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            highlight_background: "#3daee9".into(),
            highlight_foreground: "#ffffff".into(),
            link: "#3daee9".into(),
            code_background: "#232629".into(),
            border: "#7f8c8d".into(),
        }
    }
}

impl Theme {
    pub fn from_colors(colors: [&str; 5]) -> Option<Self> {
        if !colors.iter().all(|color| {
            color.len() == 7
                && color.starts_with('#')
                && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            return None;
        }
        Some(Self {
            highlight_background: colors[0].into(),
            highlight_foreground: colors[1].into(),
            link: colors[2].into(),
            code_background: colors[3].into(),
            border: colors[4].into(),
        })
    }
}

#[derive(Default)]
pub struct Document {
    source: Zeroizing<String>,
    revision: u64,
    revealed: BTreeSet<usize>,
    available: BTreeSet<usize>,
    theme: Theme,
}

pub struct RenderedDocument {
    pub html: Zeroizing<String>,
    pub actions: Vec<String>,
    pub revealed_actions: Vec<String>,
}

impl Document {
    /// Even an identical source starts a new revision with all spoilers hidden.
    pub fn load(&mut self, source: &str) -> Result<(), &'static str> {
        self.clear();
        if source.len() > MAX_SOURCE_BYTES {
            return Err("El documento Markdown supera el límite de 256 KiB.");
        }
        self.source.push_str(source);
        Ok(())
    }

    pub fn clear(&mut self) {
        self.source = Zeroizing::new(String::new());
        self.revision = NEXT_REVISION.fetch_add(1, Ordering::Relaxed);
        self.revealed.clear();
        self.available.clear();
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn toggle(&mut self, action: &str) -> bool {
        let Some((revision, index)) = action
            .strip_prefix(ACTION_SCHEME)
            .and_then(|value| value.split_once(':'))
        else {
            return false;
        };
        let (Ok(revision), Ok(index)) = (revision.parse::<u64>(), index.parse::<usize>()) else {
            return false;
        };
        if revision != self.revision || !self.available.contains(&index) {
            return false;
        }
        if !self.revealed.remove(&index) {
            self.revealed.insert(index);
        }
        true
    }

    pub fn hide_all(&mut self) {
        self.revealed.clear();
    }

    pub fn render(&mut self) -> Result<RenderedDocument, fmt::Error> {
        let arena = Arena::new();
        let options = profile();
        let root = parse_document(&arena, &self.source, &options);
        let indices = root
            .descendants()
            .filter(|node| matches!(node.data.borrow().value, NodeValue::SpoileredText))
            .enumerate()
            .map(|(index, node)| (node as *const _ as usize, index))
            .collect();
        let state = RenderState {
            revision: self.revision,
            indices,
            revealed: self.revealed.clone(),
            visible: Vec::new(),
            theme: self.theme.clone(),
        };
        let mut html = Zeroizing::new(String::new());
        let state = QtFormatter::format_document(root, &options, &mut *html, state)?;
        self.available = state.visible.iter().copied().collect();
        // A hidden ancestor also resets any revealed descendants.
        self.revealed.retain(|index| self.available.contains(index));
        let actions = state
            .visible
            .iter()
            .map(|index| action(self.revision, *index))
            .collect();
        let revealed_actions = self
            .revealed
            .iter()
            .map(|index| action(self.revision, *index))
            .collect();
        Ok(RenderedDocument {
            html,
            actions,
            revealed_actions,
        })
    }
}

fn action(revision: u64, index: usize) -> String {
    format!("{ACTION_SCHEME}{revision}:{index}")
}

fn profile() -> Options<'static> {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    options.extension.spoiler = true;
    options.extension.highlight = true;
    // Underline stays disabled: __text__ is ordinary CommonMark strong emphasis.
    options
}

/// These links are emitted as requests only; the view never launches a URL.
pub fn allowed_link(url: &str) -> bool {
    !url.chars().any(char::is_control)
        && url.split_once(':').is_some_and(|(scheme, _)| {
            ["https", "http", "mailto"]
                .iter()
                .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
        })
}

pub struct RenderState {
    revision: u64,
    indices: HashMap<usize, usize>,
    revealed: BTreeSet<usize>,
    visible: Vec<usize>,
    theme: Theme,
}

create_formatter!(QtFormatter<RenderState>, {
    NodeValue::SpoileredText => |context, node, entering| {
        let index = context.user.indices[&(node as *const _ as usize)];

        if entering {
            context.user.visible.push(index);

            if !context.user.revealed.contains(&index) {
                let url = action(context.user.revision, index);
                let color = context.user.theme.border.clone();

                // Máscara fija: no revela la longitud del secreto.
                const MASK: &str = "████████████";

                write!(context, "<a href=\"{url}\" style=\"color:{color}\">{MASK}</a>")?;

                return Ok(ChildRendering::Skip);
            }
        } else if context.user.revealed.contains(&index) {
            let url = action(context.user.revision, index);
            let color = context.user.theme.link.clone();
            context.write_str(" ")?;
            write!(context, "<a href=\"{url}\" style=\"color:{color}\">Ocultar</a>")?;
        }
    },
    NodeValue::Highlight => |context, entering| {
        if entering {
            let theme = &context.user.theme;
            let style = format!("<span style=\"background-color:{};color:{}\">", theme.highlight_background, theme.highlight_foreground);
            context.write_str(&style)?;
        } else { context.write_str("</span>")?; }
    },
    NodeValue::Strikethrough => |context, entering| {
        context.write_str(if entering { "<s>" } else { "</s>" })?;
    },
    NodeValue::HtmlBlock(_) => |entering| {
        if entering { return Ok(ChildRendering::Skip); }
    },
    NodeValue::HtmlInline(_) => |entering| {
        if entering { return Ok(ChildRendering::Skip); }
    },
    NodeValue::Raw(_) => |entering| {
        if entering { return Ok(ChildRendering::Skip); }
    },
    NodeValue::Image(_) => |context, entering| {
        if entering {
            context.write_str("[Imagen omitida]")?;
            // Alt text can contain spoiler descendants: never flatten it into the view.
            return Ok(ChildRendering::Skip);
        }
    },
    NodeValue::Link(ref link) => |context, node, entering| {
        // Avoid nested anchors when a spoiler is inside the source link.
        if allowed_link(&link.url) && !node.descendants().any(|child| matches!(child.data.borrow().value, NodeValue::SpoileredText)) {
            if entering {
                let color = context.user.theme.link.clone();
                write!(context, "<a style=\"color:{color}\" href=\"")?;
                context.escape(&link.url)?;
                context.write_str("\">")?;
            } else { context.write_str("</a>")?; }
        }
    },
    NodeValue::TaskItem(checked) => |context, entering| {
        context.write_str(if entering { if checked.is_some() { "<li>☑ " } else { "<li>☐ " } } else { "</li>\n" })?;
    },
    NodeValue::CodeBlock(ref code) => |context, entering| {
        if entering {
            let background = context.user.theme.code_background.clone();
            write!(context, "<pre style=\"background-color:{background}\"><code>")?;
            context.escape(&code.literal)?;
            context.write_str("</code></pre>\n")?;
            return Ok(ChildRendering::Skip);
        }
    },
    NodeValue::Table(_) => |context, entering| {
        if entering {
            let border = context.user.theme.border.clone();
            writeln!(context, "<table border=\"1\" cellspacing=\"0\" cellpadding=\"5\" style=\"border-color:{border};border-collapse:collapse\">")?;
        } else { context.write_str("</table>\n")?; }
    },
});

#[cfg(test)]
#[path = "markdown_tests.rs"]
mod tests;
