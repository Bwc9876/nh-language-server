use std::{collections::HashMap, path::Path};

use anyhow::Result;
use lsp_types::{Diagnostic, DiagnosticSeverity, Range, Url, VersionedTextDocumentIdentifier};
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    planets::Planet,
    project::{Project, ProjectFile},
    systems::StarSystem,
    utils::{
        error_codes::{self, get_error_code},
        xml_range_to_diag_range,
    },
    validation::{ErrorSet, Validator},
};

type ShipLogFile = VersionedTextDocumentIdentifier;

include!("base_game_entry_ids.rs");

include!("base_game_fact_ids.rs");

#[derive(Clone, Debug)]
pub struct ID {
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

pub type IdSet = Vec<ID>;

type Vector2 = (f32, f32);

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipLogEntry {
    id: String,
    astro_object: String,
    position: Option<Vector2>,
    name: String,
    parent: Option<String>,
    is_curiosity: bool,
    sources: Vec<String>,
    curiosity: Option<String>,
}

#[derive(Default, Debug)]
pub struct ShipLogContext {
    pub astro_object_ids: IdSet,
    pub entry_ids: IdSet,
    pub entries: HashMap<String, ShipLogEntry>,
    pub position_map: HashMap<String, Vector2>,
    pub fact_ids: IdSet,
    pub system_to_relative_path: HashMap<String, Vec<String>>,
    pub relative_to_astro_object: HashMap<String, String>,
    pub curiosity_references: IdSet,
    pub source_id_references: IdSet,
}

impl ShipLogContext {
    fn parse_entry(
        &mut self,
        log_file: &ShipLogFile,
        ao_id: &str,
        tree: &Document,
        node: &Node,
        parent: Option<&str>,
    ) {
        let mut entry = ShipLogEntry::default();
        entry.astro_object = ao_id.to_string();
        entry.parent = parent.map(|s| s.to_string());
        for node in node.children().filter(|n| n.is_element()) {
            match node.tag_name().name() {
                "ID" => {
                    self.entry_ids.push(ID::new(tree, &node, log_file));
                    entry.id = node.text().unwrap_or_default().to_string();
                }
                "Name" => {
                    entry.name = node.text().unwrap_or_default().to_string();
                }
                "IsCuriosity" => {
                    entry.is_curiosity = true;
                }
                "Curiosity" => {
                    self.curiosity_references
                        .push(ID::new(tree, &node, log_file));
                    entry.curiosity = Some(node.text().unwrap_or_default().to_string());
                }
                "RumorFact" | "ExploreFact" => {
                    if let Some(node) = node.children().find(|n| n.tag_name().name() == "ID") {
                        self.fact_ids.push(ID::new(tree, &node, log_file));
                    }
                    if let Some(node) = node.children().find(|n| n.tag_name().name() == "SourceID")
                    {
                        self.source_id_references
                            .push(ID::new(tree, &node, log_file));
                        entry
                            .sources
                            .push(node.text().unwrap_or_default().to_string());
                    }
                }
                "Entry" => {
                    self.parse_entry(log_file, ao_id, tree, &node, Some(&entry.id));
                }
                _ => {}
            }
        }
        if !entry.id.is_empty() {
            entry.position = self.position_map.get(&entry.id).cloned();
            if entry.name.is_empty() {
                entry.name = "UNNAMED".to_string();
            }
            self.entries.insert(entry.id.clone(), entry);
        }
    }

    pub fn parse(
        &mut self,
        log_file: &ShipLogFile,
        project_file: &ProjectFile,
        root_path: &Path,
        raw_str: &str,
    ) -> Result<()> {
        let tree = Document::parse(raw_str)?;
        let mut id = String::new();
        if let Some(node) = tree
            .descendants()
            .find(|e| e.tag_name().name() == "AstroObjectEntry")
        {
            for node in node.children().filter(|n| n.is_element()) {
                match node.tag_name().name() {
                    "ID" => {
                        id = node.text().unwrap_or_default().to_string();
                        self.astro_object_ids.push(ID::new(&tree, &node, log_file));
                        if let Some(relative_path) = project_file.get_relative(root_path) {
                            self.relative_to_astro_object
                                .insert(relative_path.to_string_lossy().to_string(), id.clone());
                        }
                    }
                    "Entry" => {
                        self.parse_entry(log_file, &id, &tree, &node, None);
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    pub fn parse_system_positions(&mut self, config: &ProjectFile) {
        let system = serde_json::from_str::<StarSystem>(&config.contents);
        match system {
            Ok(system) => {
                if let Some(positions) = system.entry_positions {
                    for entry in positions.iter() {
                        self.position_map
                            .insert(entry.id.clone(), (entry.position.x, entry.position.y));
                    }
                }
            }
            Err(why) => {
                eprintln!("Error parsing system file, ignoring: {why:?}");
            }
        }
    }

    pub fn parse_planet(&mut self, config: &ProjectFile) {
        let planet = serde_json::from_str::<Planet>(&config.contents);
        match planet {
            Ok(planet) => {
                let xml_file = planet.ShipLog.and_then(|m| m.xml_file.clone());
                if let Some(xml_file) = xml_file {
                    self.system_to_relative_path
                        .entry(planet.starSystem)
                        .or_insert_with(Vec::new)
                        .push(xml_file);
                }
            }
            Err(why) => {
                eprintln!("Error parsing planet file, ignoring: {why:?}");
            }
        }
    }

    pub fn from_project(project: &Project) -> Self {
        let mut ctx = Self::default();
        for file in project.system_files.iter() {
            ctx.parse_system_positions(&file);
        }
        for file in project.planet_files.iter() {
            ctx.parse_planet(&file);
        }
        for file in project.ship_log_files.iter() {
            let res = ctx.parse(&file.id, &file, &project.root_path, &file.contents);
            if let Err(why) = res {
                eprintln!("Error parsing ship log file: {why:?}");
            }
        }
        let vanilla: Vec<ShipLogEntry> = serde_json::from_str(include_str!("./base_game.json"))
            .expect("Failed to parse vanilla ship log entries");
        ctx.entries
            .extend(vanilla.into_iter().map(|entry| (entry.id.clone(), entry)));
        ctx
    }

    fn process_duplicate_buffer(errors: &mut ErrorSet, id_name: &str, buffer: &[&ID]) {
        errors.extend(buffer.iter().map(|id| {
            let message = format!("Duplicate {id_name} ID: `{}`", id.value);
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

    fn validate_id_set_duplicates(&self, errors: &mut ErrorSet, id_name: &str, set: &IdSet) {
        let mut set = set.clone();
        let mut current_buffer: Vec<&ID> = vec![];
        set.sort_unstable_by_key(|a| a.value.to_string());
        for id in set.iter() {
            if current_buffer
                .last()
                .map(|last_id| id.value == last_id.value)
                .unwrap_or(false)
            {
                current_buffer.push(id);
            } else {
                if current_buffer.len() > 1 {
                    Self::process_duplicate_buffer(errors, id_name, &current_buffer)
                }
                current_buffer.clear();
                current_buffer.push(id);
            }
        }
        if current_buffer.len() > 1 {
            Self::process_duplicate_buffer(errors, id_name, &current_buffer)
        }
    }

    fn validate_id_taken(
        &self,
        errors: &mut ErrorSet,
        id_name: &str,
        set: &IdSet,
        vanilla: &[&str],
    ) {
        for id in set.iter() {
            if vanilla.contains(&id.value.as_str()) {
                let message = format!("{id_name} ID `{}` is taken by the base-game", id.value);
                errors.push((
                    id.source_file.clone(),
                    Diagnostic {
                        range: id.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: get_error_code(error_codes::SHIPLOG_VANILLA_ID),
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

    fn validate_curiosity_references(&self, system_files: &[ProjectFile], errors: &mut ErrorSet) {
        const KNOWN_CURIOSITIES: [&str; 7] = [
            "None",
            "QuantumMoon",
            "SunkenModule",
            "Vessel",
            "TimeLoop",
            "CometCore",
            "InvisiblePlanet",
        ];

        let mut custom_curiosities: Vec<String> = vec![];

        for file in system_files.iter() {
            if let Ok(contents) = serde_json::from_str::<Value>(&file.contents) {
                if let Some(Some(values)) = contents.get("curiosities").map(|v| v.as_array()) {
                    custom_curiosities.extend(
                        values
                            .iter()
                            .filter_map(|v| v.get("id"))
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_string()),
                    );
                }
            }
        }

        for reference in self.curiosity_references.iter() {
            if !KNOWN_CURIOSITIES.contains(&reference.value.as_str())
                && !custom_curiosities.contains(&reference.value)
            {
                let message = format!(
                    "Unknown Curiosity: `{}`. Please define it in a system config",
                    reference.value
                );
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
            if !flattened_entry_ids.contains(&&reference.value)
                && !VANILLA_ENTRY_IDS.contains(&reference.value.as_str())
            {
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

    pub fn validate(&self, project: &Project) -> ErrorSet {
        let mut errors: ErrorSet = vec![];

        self.validate_id_set_duplicates(&mut errors, "Astro Object", &self.astro_object_ids);
        self.validate_id_set_duplicates(&mut errors, "Entry", &self.entry_ids);
        self.validate_id_set_duplicates(&mut errors, "Fact", &self.fact_ids);

        self.validate_id_taken(&mut errors, "Entry", &self.entry_ids, &VANILLA_ENTRY_IDS);
        self.validate_id_taken(&mut errors, "Fact", &self.fact_ids, &VANILLA_FACT_IDS);

        self.validate_curiosity_references(&project.system_files, &mut errors);
        self.validate_source_ids(&mut errors);

        errors
    }

    const VANILLA_ASTRO_OBJECTS: [&'static str; 14] = [
        "SUN_STATION",
        "CAVE_TWIN",
        "TOWER_TWIN",
        "TIMBER_HEARTH",
        "TIMBER_MOON",
        "BRITTLE_HOLLOW",
        "VOLCANIC_MOON",
        "GIANTS_DEEP",
        "ORBITAL_PROBE_CANNON",
        "DARK_BRAMBLE",
        "WHITE_HOLE",
        "COMET",
        "QUANTUM_MOON",
        "INVISIBLE_PLANET",
    ];

    pub fn get_entries_for_system(&self, system: &str) -> Option<Vec<&ShipLogEntry>> {
        let paths = self.system_to_relative_path.get(system)?;
        eprintln!("PATHS: {:?}", paths);
        let mut ao_ids = paths
            .iter()
            .filter_map(|path| self.relative_to_astro_object.get(path))
            .map(|s| s.as_str())
            .collect::<Vec<_>>();

        ao_ids.extend(Self::VANILLA_ASTRO_OBJECTS.iter());

        eprintln!("AO IDS: {:?}", ao_ids);
        Some(
            self.entries
                .iter()
                .filter_map(|(_id, entry)| {
                    if ao_ids.contains(&entry.astro_object.as_str()) {
                        Some(entry)
                    } else {
                        None
                    }
                })
                .collect(),
        )
    }
}

#[derive(Default)]
pub struct ShipLogValidator();

impl Validator for ShipLogValidator {
    fn prepare() -> Self {
        Self()
    }

    fn should_invalidate(&self, changed_paths: &[Url], project: &Project) -> bool {
        project
            .ship_log_files
            .iter()
            .chain(project.system_files.iter())
            .any(|file| changed_paths.contains(&file.id.uri))
    }

    fn validate(&self, project: &Project) -> Vec<(VersionedTextDocumentIdentifier, Diagnostic)> {
        ShipLogContext::from_project(project).validate(project)
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Url};
    use serde_json::json;

    use super::*;

    fn get_test_file() -> Vec<ProjectFile> {
        let contents = json!({
            "curiosities": [{
                "id": "EXAMPLE_ENTRY"
            }]
        });
        let new_file = ProjectFile::new(
            Url::parse("file://test_system.json").unwrap(),
            0,
            serde_json::to_string(&contents).unwrap(),
        );
        vec![new_file]
    }

    fn get_test_project() -> Project {
        Project {
            system_files: get_test_file(),
            ..Default::default()
        }
    }

    #[test]
    fn test_parse_example() {
        const TEST_STR: &str = include_str!("test_files/test_ship_log.xml");

        let mut ctx = ShipLogContext::default();

        let test_file = ShipLogFile::new(Url::parse("file://test_file.xml").unwrap(), 0);
        let pf = ProjectFile::dummy();
        let cwd = Path::new(".");
        ctx.parse(&test_file, &pf, cwd, TEST_STR).unwrap();

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

        let pf = ProjectFile::dummy();
        let cwd = Path::new(".");
        ctx.parse(&test_file, &pf, cwd, TEST_STR).unwrap();

        let errors = ctx.validate(&get_test_project());

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

        let pf = ProjectFile::dummy();
        let cwd = Path::new(".");
        ctx.parse(&test_file, &pf, cwd, TEST_STR).unwrap();

        let errors = ctx.validate(&get_test_project());

        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].1.message,
            "Unknown Curiosity: `COOL_ROCK`. Please define it in a system config"
        );
    }

    #[test]
    fn test_validate_missing_source_id() {
        const TEST_STR: &str = include_str!("test_files/missing_source_id.xml");

        let mut ctx = ShipLogContext::default();

        let test_file = ShipLogFile::new(Url::parse("file://test_file.xml").unwrap(), 0);

        let pf = ProjectFile::dummy();
        let cwd = Path::new(".");
        ctx.parse(&test_file, &pf, cwd, TEST_STR).unwrap();

        let errors = ctx.validate(&get_test_project());

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].1.message, "Unknown Entry: `GABAGOOL`");
    }
}
