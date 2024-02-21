use std::collections::HashMap;

#[derive(Debug, PartialEq, Default, Clone)]
pub struct CMakeBuildSpec {
    pub cmake_lists_content: Option<String>,
    pub variables: HashMap<String, String>,
}
