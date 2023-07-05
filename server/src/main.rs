use std::path::PathBuf;

use anyhow::Result;
use lsp_server::{Connection, Message};
use lsp_types::{
    notification::{DidChangeTextDocument, DidOpenTextDocument, Notification},
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, InitializeParams, OneOf,
    PositionEncodingKind, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    VersionedTextDocumentIdentifier, WorkspaceFoldersServerCapabilities,
    WorkspaceServerCapabilities,
};
use serde_json::Value;
use validation::MainValidator;

use crate::project::Project;

mod project;
mod ship_log;
mod utils;
mod validation;

fn main_loop(connection: Connection, params: Value) -> Result<()> {
    let params: InitializeParams = serde_json::from_value(params).unwrap();
    let validator = MainValidator();
    if let Some(root_uri) = params.root_uri {
        let mut project = Project::default();
        project.load_from(&PathBuf::from(root_uri.path()));
        eprintln!("Performing initial validation");
        validator.force_validate(&connection, &mut project);
        eprintln!("Starting main event loop");
        for msg in &connection.receiver {
            match msg {
                Message::Request(req) => {
                    if connection.handle_shutdown(&req)? {
                        return Ok(());
                    }
                }
                Message::Response(_) => {}
                Message::Notification(not) => match not.method.as_str() {
                    DidOpenTextDocument::METHOD => {
                        let params: DidOpenTextDocumentParams =
                            serde_json::from_value(not.params).unwrap();
                        project.open_file(
                            VersionedTextDocumentIdentifier::new(
                                params.text_document.uri.clone(),
                                params.text_document.version,
                            ),
                            &params.text_document.text,
                        );
                        validator.on_change(
                            &connection,
                            vec![params.text_document.uri],
                            &mut project,
                        );
                    }
                    DidChangeTextDocument::METHOD => {
                        let params: DidChangeTextDocumentParams =
                            serde_json::from_value(not.params).unwrap();
                        project.open_file(
                            VersionedTextDocumentIdentifier::new(
                                params.text_document.uri.clone(),
                                params.text_document.version,
                            ),
                            &params.content_changes.first().unwrap().text,
                        );
                        validator.on_change(
                            &connection,
                            vec![params.text_document.uri],
                            &mut project,
                        );
                    }
                    _ => {}
                },
            }
        }
    }
    Ok(())
}

pub fn main() -> Result<()> {
    let (connection, _) = Connection::stdio();

    let mut capabilities = ServerCapabilities::default();

    capabilities.position_encoding = Some(PositionEncodingKind::UTF16);
    capabilities.workspace = Some(WorkspaceServerCapabilities {
        workspace_folders: Some(WorkspaceFoldersServerCapabilities {
            supported: Some(true),
            change_notifications: Some(OneOf::Left(true)),
        }),
        file_operations: None,
    });
    capabilities.text_document_sync =
        Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL));

    let server_capabilities = serde_json::to_value(capabilities).unwrap();
    let initialization_params = connection.initialize(server_capabilities)?;

    main_loop(connection, initialization_params)?;

    Ok(())
}
