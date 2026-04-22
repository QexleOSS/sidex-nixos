//! Code action engine wrapping LSP `textDocument/codeAction`.
//!
//! Provides quick fixes, refactorings, and source organizers from the
//! language server.

use std::str::FromStr;

use anyhow::{Context, Result};
use lsp_types::{
    CodeActionContext, CodeActionOrCommand, CodeActionParams, Diagnostic, PartialResultParams,
    TextDocumentIdentifier, Uri, WorkDoneProgressParams,
};
use serde::{Deserialize, Serialize};

use crate::client::LspClient;
use crate::conversion::{lsp_to_range, range_to_lsp};
use crate::rename_engine::{TextEditInfo, WorkspaceEdit};

/// The kind of code action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeActionKind {
    QuickFix,
    Refactor,
    RefactorExtract,
    RefactorInline,
    RefactorRewrite,
    Source,
    SourceOrganizeImports,
    Other,
}

impl CodeActionKind {
    fn from_lsp(kind: &lsp_types::CodeActionKind) -> Self {
        let s = kind.as_str();
        match s {
            "quickfix" => Self::QuickFix,
            "refactor" => Self::Refactor,
            "refactor.extract" => Self::RefactorExtract,
            "refactor.inline" => Self::RefactorInline,
            "refactor.rewrite" => Self::RefactorRewrite,
            "source" => Self::Source,
            "source.organizeImports" => Self::SourceOrganizeImports,
            _ => Self::Other,
        }
    }
}

/// An LSP command that the server can execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// Human-readable title.
    pub title: String,
    /// The command identifier.
    pub command: String,
    /// Optional arguments as JSON values.
    pub arguments: Vec<serde_json::Value>,
}

/// Information about a code action returned to the editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeActionInfo {
    /// Human-readable title (e.g. `"Import 'HashMap'"`).
    pub title: String,
    /// The kind of code action.
    pub kind: CodeActionKind,
    /// Optional workspace edit to apply.
    pub edit: Option<WorkspaceEdit>,
    /// Optional command to execute.
    pub command: Option<Command>,
    /// Whether this is the preferred action for its diagnostics.
    pub is_preferred: bool,
}

/// Requests code actions from the language server for a given range and
/// set of diagnostics.
pub async fn request_code_actions(
    client: &LspClient,
    uri: &str,
    range: sidex_text::Range,
    diagnostics: &[Diagnostic],
) -> Result<Vec<CodeActionInfo>> {
    let lsp_range = range_to_lsp(range);
    let params = CodeActionParams {
        text_document: TextDocumentIdentifier::new(Uri::from_str(uri).context("invalid URI")?),
        range: lsp_range,
        context: CodeActionContext {
            diagnostics: diagnostics.to_vec(),
            only: None,
            trigger_kind: None,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let val = serde_json::to_value(params)?;
    let result = client
        .raw_request("textDocument/codeAction", Some(val))
        .await?;

    if result.is_null() {
        return Ok(vec![]);
    }

    let actions: Vec<CodeActionOrCommand> =
        serde_json::from_value(result).context("failed to parse code actions")?;

    Ok(actions.into_iter().map(convert_action).collect())
}

fn convert_action(action: CodeActionOrCommand) -> CodeActionInfo {
    match action {
        CodeActionOrCommand::Command(cmd) => CodeActionInfo {
            title: cmd.title.clone(),
            kind: CodeActionKind::Other,
            edit: None,
            command: Some(Command {
                title: cmd.title,
                command: cmd.command,
                arguments: cmd.arguments.unwrap_or_default(),
            }),
            is_preferred: false,
        },
        CodeActionOrCommand::CodeAction(ca) => {
            let kind = ca
                .kind
                .as_ref()
                .map_or(CodeActionKind::Other, CodeActionKind::from_lsp);

            let edit = ca.edit.map(convert_workspace_edit);

            let command = ca.command.map(|cmd| Command {
                title: cmd.title,
                command: cmd.command,
                arguments: cmd.arguments.unwrap_or_default(),
            });

            CodeActionInfo {
                title: ca.title,
                kind,
                edit,
                command,
                is_preferred: ca.is_preferred.unwrap_or(false),
            }
        }
    }
}

fn convert_workspace_edit(edit: lsp_types::WorkspaceEdit) -> WorkspaceEdit {
    let mut changes = std::collections::HashMap::new();

    if let Some(raw_changes) = edit.changes {
        for (uri, edits) in raw_changes {
            let converted: Vec<TextEditInfo> = edits
                .into_iter()
                .map(|e| TextEditInfo {
                    range: lsp_to_range(e.range),
                    new_text: e.new_text,
                })
                .collect();
            changes.insert(uri.to_string(), converted);
        }
    }

    WorkspaceEdit { changes }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_action_kind_from_lsp() {
        assert_eq!(
            CodeActionKind::from_lsp(&lsp_types::CodeActionKind::QUICKFIX),
            CodeActionKind::QuickFix
        );
        assert_eq!(
            CodeActionKind::from_lsp(&lsp_types::CodeActionKind::REFACTOR),
            CodeActionKind::Refactor
        );
        assert_eq!(
            CodeActionKind::from_lsp(&lsp_types::CodeActionKind::REFACTOR_EXTRACT),
            CodeActionKind::RefactorExtract
        );
        assert_eq!(
            CodeActionKind::from_lsp(&lsp_types::CodeActionKind::REFACTOR_INLINE),
            CodeActionKind::RefactorInline
        );
        assert_eq!(
            CodeActionKind::from_lsp(&lsp_types::CodeActionKind::SOURCE),
            CodeActionKind::Source
        );
        assert_eq!(
            CodeActionKind::from_lsp(&lsp_types::CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
            CodeActionKind::SourceOrganizeImports
        );
    }

    #[test]
    fn code_action_kind_unknown() {
        let kind = lsp_types::CodeActionKind::new("custom.kind");
        assert_eq!(CodeActionKind::from_lsp(&kind), CodeActionKind::Other);
    }

    #[test]
    fn convert_command_action() {
        let action = CodeActionOrCommand::Command(lsp_types::Command {
            title: "Run Tests".into(),
            command: "test.run".into(),
            arguments: Some(vec![serde_json::json!({"all": true})]),
        });
        let info = convert_action(action);
        assert_eq!(info.title, "Run Tests");
        assert!(info.command.is_some());
        assert_eq!(info.command.unwrap().command, "test.run");
        assert!(info.edit.is_none());
    }

    #[test]
    fn convert_code_action_with_edit() {
        let mut raw_changes = std::collections::HashMap::new();
        raw_changes.insert(
            "file:///test.rs".parse::<Uri>().unwrap(),
            vec![lsp_types::TextEdit {
                range: lsp_types::Range::new(
                    lsp_types::Position::new(0, 0),
                    lsp_types::Position::new(0, 0),
                ),
                new_text: "use std::collections::HashMap;\n".into(),
            }],
        );
        let action = CodeActionOrCommand::CodeAction(lsp_types::CodeAction {
            title: "Import HashMap".into(),
            kind: Some(lsp_types::CodeActionKind::QUICKFIX),
            diagnostics: None,
            edit: Some(lsp_types::WorkspaceEdit {
                changes: Some(raw_changes),
                document_changes: None,
                change_annotations: None,
            }),
            command: None,
            is_preferred: Some(true),
            disabled: None,
            data: None,
        });
        let info = convert_action(action);
        assert_eq!(info.title, "Import HashMap");
        assert_eq!(info.kind, CodeActionKind::QuickFix);
        assert!(info.is_preferred);
        assert!(info.edit.is_some());
    }

    #[test]
    fn code_action_info_serialize() {
        let info = CodeActionInfo {
            title: "Fix import".into(),
            kind: CodeActionKind::QuickFix,
            edit: None,
            command: None,
            is_preferred: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: CodeActionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title, "Fix import");
        assert!(back.is_preferred);
    }

    #[test]
    fn command_serialize() {
        let cmd = Command {
            title: "Run".into(),
            command: "run.cmd".into(),
            arguments: vec![serde_json::json!(42)],
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let back: Command = serde_json::from_str(&json).unwrap();
        assert_eq!(back.command, "run.cmd");
    }
}
