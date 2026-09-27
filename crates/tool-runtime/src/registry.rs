//! Tool registry — maps tool names to implementations.

use crate::tools::*;
use std::collections::HashMap;

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };
        registry.register(Box::new(ReadFile));
        registry.register(Box::new(WriteFile));
        registry.register(Box::new(ListDirectory));
        registry.register(Box::new(SearchFiles));
        registry.register(Box::new(RunCommand));
        registry.register(Box::new(GetSystemInfo));
        registry.register(Box::new(GetNetworkStatus));
        registry.register(Box::new(ManageService));
        registry.register(Box::new(PartitionDisk));
        registry
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn list(&self) -> Vec<(&str, &str, niat_common::types::SafetyLevel)> {
        self.tools.values()
            .map(|t| (t.name(), t.description(), t.safety_level()))
            .collect()
    }
}
