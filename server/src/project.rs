use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use glob::glob;
use lsp_types::{Url, VersionedTextDocumentIdentifier};

#[derive(Debug)]
pub struct ProjectFile {
    pub id: VersionedTextDocumentIdentifier,
    pub contents: String,
}

impl ProjectFile {
    pub fn new(url: Url, version: i32, contents: String) -> Self {
        Self {
            id: VersionedTextDocumentIdentifier {
                uri: url,
                version: version,
            },
            contents,
        }
    }
}

type ProjectFiles = Vec<ProjectFile>;

#[derive(Default, Debug)]
pub struct Project {
    pub root_path: PathBuf,

    pub planet_files: ProjectFiles,
    pub system_files: ProjectFiles,
    pub ship_log_files: ProjectFiles,
    pub dialogue_files: ProjectFiles,
    pub text_files: ProjectFiles,
    pub files_with_diagnostics: Vec<Url>,
}

impl Project {
    fn read_project_file(files: &mut ProjectFiles, path: &Path) {
        let raw_url = format!("file://{}", path.to_str().unwrap());
        let url = Url::parse(&raw_url);

        if let Ok(url) = url {
            let contents = fs::read_to_string(path);

            match contents {
                Ok(contents) => files.push(ProjectFile::new(url, 0, contents)),
                Err(why) => {
                    eprintln!("Failed to read {raw_url}: {why:?}");
                }
            }
        }
    }

    fn crawl_folder(files: &mut ProjectFiles, path: &Path, folder: &str) {
        for entry in glob(
            path.join(folder)
                .join("**")
                .join("*.json")
                .to_str()
                .unwrap(),
        )
        .unwrap()
        {
            if let Ok(entry) = entry {
                Self::read_project_file(files, &entry.as_path());
            }
        }
    }

    fn find_planets(&mut self, path: &Path) {
        Self::crawl_folder(&mut self.planet_files, path, "planets");
    }

    fn find_systems(&mut self, path: &Path) {
        Self::crawl_folder(&mut self.system_files, path, "systems");
    }

    fn find_ship_logs(&mut self, path: &Path) {
        for file in self.planet_files.iter() {
            let json: Result<serde_json::Value, _> = serde_json::from_str(&file.contents);
            if let Ok(json) = json {
                let xml_file = json.pointer("/ShipLog/xmlFile").map(|vv| vv.as_str());
                if let Some(Some(xml_file)) = xml_file {
                    Self::read_project_file(&mut self.ship_log_files, &path.join(xml_file))
                }
            }
        }
    }

    fn find_dialogue(&mut self, path: &Path) {
        for file in self.planet_files.iter() {
            let json: Result<serde_json::Value, _> = serde_json::from_str(&file.contents);
            if let Ok(json) = json {
                let arr = json.pointer("/Props/dialogue").map(|a| a.as_array());
                if let Some(Some(arr)) = arr {
                    for value in arr.iter().filter(|v| v.is_object()) {
                        if let Some(Some(xml_file)) = value.get("xmlFile").map(|v| v.as_str()) {
                            Self::read_project_file(&mut self.dialogue_files, &path.join(xml_file))
                        }
                    }
                }
            }
        }
    }

    fn find_text(&mut self, path: &Path) {
        for file in self.planet_files.iter() {
            let json: Result<serde_json::Value, _> = serde_json::from_str(&file.contents);
            if let Ok(json) = json {
                let arr = json.pointer("/Props/translatorText").map(|a| a.as_array());
                if let Some(Some(arr)) = arr {
                    for value in arr.iter().filter(|v| v.is_object()) {
                        if let Some(Some(xml_file)) = value.get("xmlFile").map(|v| v.as_str()) {
                            Self::read_project_file(&mut self.text_files, &path.join(xml_file))
                        }
                    }
                }
            }
        }
        for file in self.planet_files.iter() {
            let json: Result<serde_json::Value, _> = serde_json::from_str(&file.contents);
            if let Ok(json) = json {
                let arr = json.pointer("/Props/remotes").map(|a| a.as_array());
                if let Some(Some(arr)) = arr {
                    for value in arr.iter().filter(|v| v.is_object()) {
                        if let Some(Some(xml_file)) = value
                            .get("whiteboard/nomaiText/xmlFile")
                            .map(|v| v.as_str())
                        {
                            Self::read_project_file(&mut self.text_files, &path.join(xml_file))
                        }
                    }
                }
            }
        }
    }

    // TODO: Nomai Text Loading, don't feel like it rn
    // Props/translatorText/*/xmlFile
    // Props/remotes/*/whiteboard/nomaiText/xmlFile

    pub fn load_from(&mut self, path: &Path) {
        self.root_path = path.to_owned();

        eprintln!("Begin Project Discovery");

        let now = Instant::now();

        self.find_planets(path);

        eprintln!("Found {} Planets", self.planet_files.len());

        self.find_systems(path);

        eprintln!("Found {} Star Systems", self.system_files.len());

        self.find_ship_logs(path);

        eprintln!("Found {} Ship Logs", self.ship_log_files.len());

        self.find_dialogue(path);

        eprintln!("Found {} Dialogue Trees", self.dialogue_files.len());

        self.find_text(path);

        eprintln!("Found {} Nomai Text Definitions", self.text_files.len());

        eprintln!("Project Discovery Complete in {:?}", now.elapsed());
    }

    fn check_file_add(
        files: &mut ProjectFiles,
        id: &VersionedTextDocumentIdentifier,
        contents: &str,
    ) -> bool {
        for file in files.iter_mut() {
            if id.uri == file.id.uri && id.version > file.id.version {
                file.id = id.clone();
                file.contents = contents.to_string();
                return true;
            }
        }
        false
    }

    pub fn open_file(&mut self, id: VersionedTextDocumentIdentifier, contents: &str) {
        for files in [
            &mut self.dialogue_files,
            &mut self.ship_log_files,
            &mut self.system_files,
            &mut self.planet_files,
            &mut self.text_files,
        ] {
            if Self::check_file_add(files, &id, contents) {
                break;
            }
        }
    }
}
