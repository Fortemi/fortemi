// Sample Rust code for document type detection testing
// This file should be detected as type: rust-source

use std::collections::HashMap;

/// A sample struct for testing
#[derive(Debug, Clone)]
pub struct UatTestStruct {
    pub id: u64,
    pub name: String,
    pub data: HashMap<String, String>,
}

impl UatTestStruct {
    /// Create a new instance
    pub fn new(id: u64, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            data: HashMap::new(),
        }
    }

    /// Add data to the struct
    pub fn add_data(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let s = UatTestStruct::new(1, "test");
        assert_eq!(s.id, 1);
        assert_eq!(s.name, "test");
    }
}
