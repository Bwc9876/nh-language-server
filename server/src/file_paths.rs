use json_position_parser::tree::EntryType;
use lsp_types::{Diagnostic, DiagnosticSeverity};
use serde_json::Value;

use crate::{
    project::{Project, ProjectFile},
    utils::{
        error_codes::{self, get_error_code},
        find_paths_with_x_prop, json_path_to_json_pos_path, json_pos_range_to_diag_range,
    },
    validation::{ErrorSet, Validator},
};

type JsonPathSet = Vec<String>;

#[derive(Debug, Default)]
pub struct FilePathValidator {
    body_schema_file_paths: JsonPathSet,
}

impl FilePathValidator {
    fn prepare_from_schema(url: &str, files: &mut JsonPathSet) {
        if let Ok(Ok(schema)) = reqwest::blocking::get(url).map(|r| r.text()) {
            if let Ok(schema) = serde_json::from_str::<Value>(&schema) {
                files.extend(find_paths_with_x_prop("x-file-path", "", &schema, &schema));
            }
        }
    }

    fn validate_file_or_folder_paths(
        &self,
        project: &Project,
        files: &[ProjectFile],
        json_paths: &[String],
        errors: &mut ErrorSet,
    ) {
        for config in files.iter() {
            let tree = json_position_parser::parse_json(&config.contents);
            if let Ok(tree) = tree {
                for path_to_check in json_paths.iter() {
                    let parsed_path = json_path_to_json_pos_path(path_to_check);
                    for found in tree.value_at(&parsed_path) {
                        if let EntryType::String(file_path) = &found.entry_type {
                            let complete_path = project.root_path.join(file_path);
                            if !complete_path.is_file() && !complete_path.is_dir() {
                                errors.push((
                                    config.id.clone(),
                                    Diagnostic {
                                        range: json_pos_range_to_diag_range(found.range),
                                        severity: Some(DiagnosticSeverity::ERROR),
                                        code: get_error_code(
                                            error_codes::CONFIG_FILE_PATH_NOT_FOUND,
                                        ),
                                        code_description: None,
                                        source: Some(error_codes::ERROR_SOURCE.to_string()),
                                        message: format!("File path {file_path} not found",),
                                        related_information: None,
                                        tags: None,
                                        data: None,
                                    },
                                ))
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Validator for FilePathValidator {
    fn prepare() -> Self {
        let mut this = Self::default();
        Self::prepare_from_schema("https://gist.github.com/Bwc9876/d54b0a1185f223cac6fdc0110832f929/raw/ca628288f4c168140bd6014ab49bfaf4f54d3f5d/test-schema.json", &mut this.body_schema_file_paths);
        this
    }

    fn should_invalidate(&self, _: &[lsp_types::Url], _: &Project) -> bool {
        // Any file changes can mean we need to reload, so always return true here
        true
    }

    fn validate(&self, project: &Project) -> ErrorSet {
        let mut errors = vec![];
        self.validate_file_or_folder_paths(
            project,
            &project.planet_files,
            &self.body_schema_file_paths,
            &mut errors,
        );
        errors
    }
}
