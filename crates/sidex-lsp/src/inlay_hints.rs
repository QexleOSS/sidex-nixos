//! Inlay hints engine wrapping LSP `textDocument/inlayHint`.
//!
//! Inlay hints show inline annotations (type hints, parameter names) in the
//! editor without modifying the document text.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::client::LspClient;
use crate::conversion::{lsp_to_position, range_to_lsp};

/// The kind of inlay hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlayHintKind {
    /// A type annotation hint (e.g. `let x /* : i32 */ = 5`).
    Type,
    /// A parameter name hint (e.g. `foo(/* name: */ "bar")`).
    Parameter,
}

/// An inlay hint to display in the editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlayHintInfo {
    /// The position in the document where the hint should be displayed.
    pub position: sidex_text::Position,
    /// The text label to display.
    pub label: String,
    /// The kind of hint.
    pub kind: InlayHintKind,
    /// Whether to add padding before the hint.
    pub padding_left: bool,
    /// Whether to add padding after the hint.
    pub padding_right: bool,
}

/// Requests inlay hints from the language server for a given range.
pub async fn request_inlay_hints(
    client: &LspClient,
    uri: &str,
    range: sidex_text::Range,
) -> Result<Vec<InlayHintInfo>> {
    let lsp_range = range_to_lsp(range);
    let hints = client.inlay_hints(uri, lsp_range).await?;

    Ok(hints.into_iter().map(convert_hint).collect())
}

fn convert_hint(hint: lsp_types::InlayHint) -> InlayHintInfo {
    let position = lsp_to_position(hint.position);

    let label = match hint.label {
        lsp_types::InlayHintLabel::String(s) => s,
        lsp_types::InlayHintLabel::LabelParts(parts) => {
            parts.into_iter().map(|p| p.value).collect::<String>()
        }
    };

    let kind = match hint.kind {
        Some(lsp_types::InlayHintKind::PARAMETER) => InlayHintKind::Parameter,
        _ => InlayHintKind::Type,
    };

    InlayHintInfo {
        position,
        label,
        kind,
        padding_left: hint.padding_left.unwrap_or(false),
        padding_right: hint.padding_right.unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_type_hint() {
        let hint = lsp_types::InlayHint {
            position: lsp_types::Position::new(5, 10),
            label: lsp_types::InlayHintLabel::String(": i32".into()),
            kind: Some(lsp_types::InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: Some(true),
            padding_right: Some(false),
            data: None,
        };
        let info = convert_hint(hint);
        assert_eq!(info.position.line, 5);
        assert_eq!(info.position.column, 10);
        assert_eq!(info.label, ": i32");
        assert_eq!(info.kind, InlayHintKind::Type);
        assert!(info.padding_left);
        assert!(!info.padding_right);
    }

    #[test]
    fn convert_parameter_hint() {
        let hint = lsp_types::InlayHint {
            position: lsp_types::Position::new(10, 5),
            label: lsp_types::InlayHintLabel::String("name:".into()),
            kind: Some(lsp_types::InlayHintKind::PARAMETER),
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: Some(true),
            data: None,
        };
        let info = convert_hint(hint);
        assert_eq!(info.label, "name:");
        assert_eq!(info.kind, InlayHintKind::Parameter);
        assert!(!info.padding_left);
        assert!(info.padding_right);
    }

    #[test]
    fn convert_label_parts() {
        let hint = lsp_types::InlayHint {
            position: lsp_types::Position::new(0, 0),
            label: lsp_types::InlayHintLabel::LabelParts(vec![
                lsp_types::InlayHintLabelPart {
                    value: ": ".into(),
                    tooltip: None,
                    location: None,
                    command: None,
                },
                lsp_types::InlayHintLabelPart {
                    value: "Vec<String>".into(),
                    tooltip: None,
                    location: None,
                    command: None,
                },
            ]),
            kind: Some(lsp_types::InlayHintKind::TYPE),
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: None,
            data: None,
        };
        let info = convert_hint(hint);
        assert_eq!(info.label, ": Vec<String>");
    }

    #[test]
    fn inlay_hint_kind_serialize() {
        let json = serde_json::to_string(&InlayHintKind::Type).unwrap();
        let back: InlayHintKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, InlayHintKind::Type);

        let json = serde_json::to_string(&InlayHintKind::Parameter).unwrap();
        let back: InlayHintKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, InlayHintKind::Parameter);
    }

    #[test]
    fn inlay_hint_info_serialize() {
        let info = InlayHintInfo {
            position: sidex_text::Position::new(1, 2),
            label: ": bool".into(),
            kind: InlayHintKind::Type,
            padding_left: true,
            padding_right: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: InlayHintInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.label, ": bool");
        assert!(back.padding_left);
        assert!(!back.padding_right);
    }

    #[test]
    fn default_kind_when_none() {
        let hint = lsp_types::InlayHint {
            position: lsp_types::Position::new(0, 0),
            label: lsp_types::InlayHintLabel::String("hint".into()),
            kind: None,
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: None,
            data: None,
        };
        let info = convert_hint(hint);
        assert_eq!(info.kind, InlayHintKind::Type);
    }
}
