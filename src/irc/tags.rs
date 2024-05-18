use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Tags {
    tags: HashMap<String, Option<String>>,
}

impl Tags {
    pub fn empty() -> Self {
        Self {
            tags: HashMap::new(),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        todo!()
    }
}
