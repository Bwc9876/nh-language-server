use json_position_parser::{tree::PathType, types::Range as JSONRange};
use lsp_types::{Position as LSPPosition, Range as LSPRange};
use roxmltree::TextPos;
use serde_json::Value;

pub mod error_codes {
    use lsp_types::NumberOrString;

    pub const ERROR_SOURCE: &str = "New Horizons";

    pub const SHIPLOG_DUPLICATE_ID: &str = "nh.shiplog.duplicate_ids";
    pub const SHIPLOG_MISSING_CURIOSITY: &str = "nh.shiplog.missing_curiosity";
    pub const SHIPLOG_MISSING_SOURCE_ID: &str = "nh.shiplog.invalid_source_id";

    pub const CONFIG_FILE_PATH_NOT_FOUND: &str = "nh.config.file_path_invalid";

    pub fn get_error_code(code: &str) -> Option<NumberOrString> {
        Some(NumberOrString::String(code.to_string()))
    }
}

pub fn xml_range_to_diag_range(start_pos: TextPos, end_pos: TextPos) -> LSPRange {
    LSPRange::new(
        LSPPosition::new(start_pos.row - 1, start_pos.col - 1),
        LSPPosition::new(end_pos.row - 1, end_pos.col - 1),
    )
}

pub fn json_pos_range_to_diag_range(range: JSONRange) -> LSPRange {
    LSPRange::new(
        LSPPosition::new(range.start.line as u32, range.start.char as u32),
        LSPPosition::new(range.end.line as u32, range.end.char as u32),
    )
}

pub fn find_paths_with_x_prop(
    x_prop: &str,
    path: &str,
    schema: &Value,
    node: &Value,
) -> Vec<String> {
    let mut paths: Vec<String> = vec![];
    let mut node = node;
    if let Some(schema_ref) = node.get("$ref") {
        let target = schema_ref.as_str().map(|s| s.split('/').last());
        if let Some(Some(target)) = target {
            if let Some(Some(new_node)) = schema.get("definitions").map(|d| d.get(target)) {
                node = new_node;
            }
        }
    }
    if let Some(Some(node_type)) = node.get("type").map(|t| t.as_str()) {
        match node_type {
            "string" => {
                if let Some(Some(flag)) = node.get(x_prop).map(|x| x.as_bool()) {
                    if flag {
                        paths.push(path.to_string())
                    }
                }
            }
            "object" => {
                if let Some(Some(props)) = node.get("properties").map(|p| p.as_object()) {
                    for (name, prop) in props {
                        paths.extend(find_paths_with_x_prop(
                            x_prop,
                            &format!("{path}/{name}"),
                            schema,
                            prop,
                        ))
                    }
                }
            }
            "array" => {
                if let Some(items) = node.get("items") {
                    paths.extend(find_paths_with_x_prop(
                        x_prop,
                        &format!("{path}/*"),
                        schema,
                        items,
                    ));
                }
            }
            _ => {}
        }
    }
    paths
}

pub fn json_path_to_json_pos_path(path: &str) -> Vec<PathType<'_>> {
    let parts = path.split('/').skip(1);
    let output_path = parts
        .into_iter()
        .map(|s| {
            if s == "*" {
                PathType::Wildcard
            } else {
                PathType::Object(s)
            }
        })
        .collect();
    output_path
}
