use anyhow::Result;
use lsp_types::{
    Diagnostic, DiagnosticSeverity, Position, Range, Url, VersionedTextDocumentIdentifier,
};
use roxmltree::{Document, Node, TextPos};

use crate::{
    project::Project,
    utils::error_codes::{self, get_error_code},
    validation::{ErrorSet, Validator},
};

type ShipLogFile = VersionedTextDocumentIdentifier;

fn xml_range_to_diag_range(start_pos: TextPos, end_pos: TextPos) -> Range {
    Range::new(
        Position::new(start_pos.row - 1, start_pos.col - 1),
        Position::new(end_pos.row - 1, end_pos.col - 1),
    )
}

#[derive(Clone, Debug)]
struct ID {
    pub value: String,
    pub source_file: ShipLogFile,
    pub range: Range,
}

impl ID {
    fn new(tree: &Document, node: &Node, log_file: &ShipLogFile) -> Self {
        Self {
            value: node.text().unwrap_or_default().to_string(),
            source_file: log_file.clone(),
            range: xml_range_to_diag_range(
                tree.text_pos_at(node.range().start),
                tree.text_pos_at(node.range().end),
            ),
        }
    }
}

type IdSet = Vec<ID>;

#[derive(Default, Debug)]
struct ShipLogContext {
    pub astro_object_ids: IdSet,
    pub entry_ids: IdSet,
    pub fact_ids: IdSet,
    pub curiosity_references: IdSet,
    pub source_id_references: IdSet,
}

impl ShipLogContext {
    fn parse_entry(&mut self, log_file: &ShipLogFile, tree: &Document, node: &Node) {
        for node in node.children().filter(|n| n.is_element()) {
            match node.tag_name().name() {
                "ID" => {
                    self.entry_ids.push(ID::new(tree, &node, log_file));
                }
                "Curiosity" => {
                    self.curiosity_references
                        .push(ID::new(tree, &node, log_file));
                }
                "RumorFact" | "ExploreFact" => {
                    if let Some(node) = node.children().find(|n| n.tag_name().name() == "ID") {
                        self.fact_ids.push(ID::new(tree, &node, log_file));
                    }
                    if let Some(node) = node.children().find(|n| n.tag_name().name() == "SourceID")
                    {
                        self.source_id_references
                            .push(ID::new(tree, &node, log_file));
                    }
                }
                "Entry" => {
                    self.parse_entry(log_file, tree, &node);
                }
                _ => {}
            }
        }
    }

    pub fn parse(&mut self, log_file: &ShipLogFile, raw_str: &str) -> Result<()> {
        let tree = Document::parse(raw_str)?;

        if let Some(node) = tree
            .descendants()
            .find(|e| e.tag_name().name() == "AstroObjectEntry")
        {
            for node in node.children().filter(|n| n.is_element()) {
                match node.tag_name().name() {
                    "ID" => {
                        self.astro_object_ids.push(ID::new(&tree, &node, log_file));
                    }
                    "Entry" => {
                        self.parse_entry(log_file, &tree, &node);
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn process_duplicate_buffer(errors: &mut ErrorSet, buffer: &Vec<&ID>) {
        errors.extend(buffer.iter().map(|id| {
            let message = format!("Duplicate ID: `{}`", id.value);
            (
                id.source_file.clone(),
                Diagnostic {
                    range: id.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: get_error_code(error_codes::SHIPLOG_DUPLICATE_ID),
                    code_description: None,
                    source: Some(error_codes::ERROR_SOURCE.to_string()),
                    message,
                    related_information: None,
                    tags: None,
                    data: None,
                },
            )
        }));
    }

    fn validate_id_set_duplicates(&self, errors: &mut ErrorSet, set: &IdSet) {
        let mut set = set.clone();
        let mut current_buffer: Vec<&ID> = vec![];
        set.sort_unstable_by_key(|a| a.value.to_string());
        for id in set.iter() {
            if current_buffer
                .last()
                .map(|last_id| id.value == last_id.value)
                .unwrap_or(false)
            {
                current_buffer.push(&id);
            } else {
                if current_buffer.len() > 1 {
                    Self::process_duplicate_buffer(errors, &current_buffer)
                }
                current_buffer.clear();
                current_buffer.push(&id);
            }
        }
        if current_buffer.len() > 1 {
            Self::process_duplicate_buffer(errors, &current_buffer)
        }
    }

    fn validate_curiosity_references(&self, errors: &mut ErrorSet) {
        // TODO: Fill this out
        const KNOWN_CURIOSITIES: [&str; 0] = [];

        let flattened_entry_ids: Vec<&String> = self.entry_ids.iter().map(|i| &i.value).collect();

        for reference in self.curiosity_references.iter() {
            if !KNOWN_CURIOSITIES.contains(&reference.value.as_str())
                && !flattened_entry_ids.contains(&&reference.value)
            {
                let message = format!("Unknown Curiosity: `{}`", reference.value);
                errors.push((
                    reference.source_file.clone(),
                    Diagnostic {
                        range: reference.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: get_error_code(error_codes::SHIPLOG_MISSING_CURIOSITY),
                        code_description: None,
                        source: Some(error_codes::ERROR_SOURCE.to_string()),
                        message,
                        related_information: None,
                        tags: None,
                        data: None,
                    },
                ))
            }
        }
    }

    fn validate_source_ids(&self, errors: &mut ErrorSet) {
        let flattened_entry_ids: Vec<&String> = self.entry_ids.iter().map(|i| &i.value).collect();

        for reference in self.source_id_references.iter() {
            if !flattened_entry_ids.contains(&&reference.value) {
                let message = format!("Unknown Entry: `{}`", reference.value);
                errors.push((
                    reference.source_file.clone(),
                    Diagnostic {
                        range: reference.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: get_error_code(error_codes::SHIPLOG_MISSING_SOURCE_ID),
                        code_description: None,
                        source: Some(error_codes::ERROR_SOURCE.to_string()),
                        message,
                        related_information: None,
                        tags: None,
                        data: None,
                    },
                ))
            }
        }
    }

    pub fn validate(&self) -> ErrorSet {
        let mut errors: ErrorSet = vec![];

        self.validate_id_set_duplicates(&mut errors, &self.astro_object_ids);
        self.validate_id_set_duplicates(&mut errors, &self.entry_ids);
        self.validate_id_set_duplicates(&mut errors, &self.fact_ids);

        self.validate_curiosity_references(&mut errors);
        self.validate_source_ids(&mut errors);

        errors
    }
}

#[derive(Default)]
pub struct ShipLogValidator();

impl Validator for ShipLogValidator {
    fn should_invalidate(&self, changed_paths: &Vec<Url>, project: &Project) -> bool {
        project
            .ship_log_files
            .iter()
            .any(|file| changed_paths.contains(&file.id.uri))
    }

    fn validate(&self, project: &Project) -> Vec<(VersionedTextDocumentIdentifier, Diagnostic)> {
        let mut ctx = ShipLogContext::default();
        for file in project.ship_log_files.iter() {
            let res = ctx.parse(&file.id, &file.contents);
            if let Err(why) = res {
                eprintln!("Error parsing ship log file: {why:?}");
            }
        }
        ctx.validate()
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::Url;

    use super::*;

    #[test]
    fn test_parse_example() {
        const TEST_STR: &str = include_str!("test_files/test_ship_log.xml");

        let mut ctx = ShipLogContext::default();

        let test_file = ShipLogFile::new(Url::parse("file://test_file.xml").unwrap(), 0);

        ctx.parse(&test_file, TEST_STR).unwrap();

        assert_eq!(ctx.astro_object_ids.len(), 1);
        assert_eq!(ctx.astro_object_ids[0].value, "EXAMPLE_PLANET");
        assert_eq!(
            ctx.astro_object_ids[0].range.start,
            Position {
                line: 2,
                character: 4
            }
        );

        assert_eq!(ctx.entry_ids.len(), 3);
        assert_eq!(ctx.entry_ids[0].value, "EXAMPLE_ENTRY");
        assert_eq!(ctx.entry_ids[1].value, "EXAMPLE_CHILD_ENTRY");
        assert_eq!(
            ctx.entry_ids[1].range.start,
            Position {
                line: 33,
                character: 12
            }
        );
    }

    #[test]
    fn test_validate_duplicates() {
        const TEST_STR: &str = include_str!("test_files/duplicate_ids.xml");

        let mut ctx = ShipLogContext::default();

        let test_file = ShipLogFile::new(Url::parse("file://test_file.xml").unwrap(), 0);

        ctx.parse(&test_file, TEST_STR).unwrap();

        let errors = ctx.validate();

        assert_eq!(errors.len(), 6);
        assert_eq!(
            errors
                .iter()
                .filter(|e| e.1.message == "Duplicate ID: `EXAMPLE_ENTRY`")
                .count(),
            2
        );
        assert_eq!(
            errors
                .iter()
                .filter(|e| e.1.message == "Duplicate ID: `EXAMPLE_EXPLORE_FACT`")
                .count(),
            2
        );
        assert_eq!(
            errors
                .iter()
                .filter(|e| e.1.message == "Duplicate ID: `EXAMPLE_RUMOR_FACT`")
                .count(),
            2
        );
    }

    #[test]
    fn test_validate_missing_curiosity() {
        const TEST_STR: &str = include_str!("test_files/missing_curiosity.xml");

        let mut ctx = ShipLogContext::default();

        let test_file = ShipLogFile::new(Url::parse("file://test_file.xml").unwrap(), 0);

        ctx.parse(&test_file, TEST_STR).unwrap();

        let errors = ctx.validate();

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].1.message, "Unknown Curiosity: `COOL_ROCK`");
    }

    #[test]
    fn test_validate_missing_source_id() {
        const TEST_STR: &str = include_str!("test_files/missing_source_id.xml");

        let mut ctx = ShipLogContext::default();

        let test_file = ShipLogFile::new(Url::parse("file://test_file.xml").unwrap(), 0);

        ctx.parse(&test_file, TEST_STR).unwrap();

        let errors = ctx.validate();

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].1.message, "Unknown Entry: `GABAGOOL`");
    }
}
