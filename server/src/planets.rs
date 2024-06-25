use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShipLogModule {
    pub xml_file: Option<String>,
}

const DEFAULT_SOLAR_SYSTEM: &str = "SolarSystem";

fn default_star_system() -> String {
    DEFAULT_SOLAR_SYSTEM.to_string()
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)] // NH Configs Silly
pub struct Planet {
    pub name: String,
    #[serde(default = "default_star_system")]
    pub starSystem: String,
    pub ShipLog: Option<ShipLogModule>,
}
