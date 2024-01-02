use std::time::Instant;

use lsp_server::{Connection, Message, Notification};
use lsp_types::{
    notification::{Notification as INotification, PublishDiagnostics},
    Diagnostic, PublishDiagnosticsParams, Url, VersionedTextDocumentIdentifier,
};

use crate::{file_paths::FilePathValidator, project::Project, ship_log::ShipLogValidator};

pub type ErrorSet = Vec<(VersionedTextDocumentIdentifier, Diagnostic)>;

pub trait Validator {
    fn prepare() -> Self
    where
        Self: Sized;
    fn should_invalidate(&self, changed_paths: &[Url], project: &Project) -> bool;
    fn validate(&self, project: &Project) -> ErrorSet;
}

#[derive(Default)]
pub struct MainValidator {
    pub validators: Vec<Box<dyn Validator>>,
}

impl MainValidator {
    pub fn new() -> Self {
        Self {
            validators: vec![
                Box::new(ShipLogValidator::prepare()),
                Box::new(FilePathValidator::prepare()),
            ],
        }
    }

    fn internal_emit(connection: &Connection, current_buffer: &ErrorSet) {
        let params = PublishDiagnosticsParams {
            uri: current_buffer.last().unwrap().0.uri.clone(),
            diagnostics: current_buffer.iter().map(|e| e.1.clone()).collect(),
            version: Some(current_buffer.last().unwrap().0.version),
        };
        let res = connection.sender.send(Message::Notification(Notification {
            method: PublishDiagnostics::METHOD.to_string(),
            params: serde_json::to_value(params).unwrap(),
        }));
        if let Err(why) = res {
            eprintln!("Error emitting diagnostics: {why:?}");
        }
    }

    fn emit_diagnostics(&self, connection: &Connection, mut errors: ErrorSet) {
        let mut current_buffer: ErrorSet = vec![];
        let mut last_uri: Option<Url> = None;
        errors.sort_unstable_by_key(|e| e.0.uri.clone());
        for error in errors.into_iter() {
            if last_uri.map(|u| u == error.0.uri).unwrap_or(true) {
                current_buffer.push(error.clone());
            } else {
                Self::internal_emit(connection, &current_buffer);
                current_buffer.clear();
                current_buffer.push(error.clone());
            }
            last_uri = Some(error.0.uri.clone());
        }
        if !current_buffer.is_empty() {
            Self::internal_emit(connection, &current_buffer);
        }
    }

    pub fn force_validate(&self, connection: &Connection, project: &mut Project) {
        let now = Instant::now();

        let mut errors: ErrorSet = vec![];
        for validator in &self.validators {
            errors.extend(validator.validate(project).into_iter());
        }

        let len = errors.len();

        project.files_with_diagnostics = errors
            .iter()
            .map(|e| e.0.clone())
            .collect::<Vec<VersionedTextDocumentIdentifier>>();

        project.files_with_diagnostics.dedup();

        self.emit_diagnostics(connection, errors);

        eprintln!(
            "Finished validation, found {} errors in {:?}",
            len,
            now.elapsed()
        );
    }

    pub fn on_change(
        &self,
        connection: &Connection,
        changed_paths: Vec<Url>,
        project: &mut Project,
    ) {
        let mut errors: ErrorSet = vec![];
        for validator in self
            .validators
            .iter()
            .filter(|v| v.should_invalidate(&changed_paths, project))
        {
            errors.extend(validator.validate(project).into_iter());
        }

        eprintln!("Validate: {:?}", errors);

        let mut uris_with_diagnostics =
            errors.iter().map(|e| e.0.uri.clone()).collect::<Vec<Url>>();

        uris_with_diagnostics.sort();
        uris_with_diagnostics.dedup();

        self.emit_diagnostics(connection, errors);

        for file in project.iter_all() {
            if !uris_with_diagnostics.contains(&file.id.uri) {
                let params = PublishDiagnosticsParams {
                    uri: file.id.uri.clone(),
                    version: project
                        .files_with_diagnostics
                        .iter()
                        .find(|f| f.uri == file.id.uri)
                        .map(|f| f.version),
                    diagnostics: vec![],
                };
                let res = connection
                    .sender
                    .send(Message::Notification(Notification::new(
                        PublishDiagnostics::METHOD.to_string(),
                        params,
                    )));
                if let Err(why) = res {
                    eprintln!("Error emitting diagnostics: {why:?}");
                }
            }
        }

        project
            .files_with_diagnostics
            .retain(|f| !changed_paths.contains(&f.uri) || uris_with_diagnostics.contains(&f.uri));
    }
}
