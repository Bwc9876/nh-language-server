pub mod error_codes {
    use lsp_types::NumberOrString;

    pub const ERROR_SOURCE: &str = "New Horizons";

    pub const SHIPLOG_DUPLICATE_ID: &str = "nh.shiplog.duplicate_ids";
    pub const SHIPLOG_MISSING_CURIOSITY: &str = "nh.shiplog.missing_curiosity";
    pub const SHIPLOG_MISSING_SOURCE_ID: &str = "nh.shiplog.invalid_source_id";

    pub fn get_error_code(code: &str) -> Option<NumberOrString> {
        Some(NumberOrString::String(code.to_string()))
    }
}
