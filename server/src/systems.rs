use serde::Deserialize;

#[derive(Deserialize)]
pub struct MVector2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Deserialize)]
pub struct EntryPos {
    pub id: String,
    pub position: MVector2,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StarSystem {
    pub entry_positions: Option<Vec<EntryPos>>,
}
