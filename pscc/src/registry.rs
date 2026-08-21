use std::collections::HashMap;

use crate::ast::{Function, Type};

#[derive(Clone, Debug, Default)]
pub struct Registry {
    pub classes: HashMap<String, ClassInfo>,
}

impl Registry {
    pub fn func(&self, class: &str, func: &str) -> Option<&Function> {
        self.classes
            .get(class)
            .and_then(|class| class.functions.get(func))
    }

    pub fn property(&self, class: &str, prop: &str) -> Option<&Type> {
        self.classes
            .get(class)
            .and_then(|class| class.properties.get(prop))
    }
}

#[derive(Clone, Debug, Default)]
pub struct ClassInfo {
    pub properties: HashMap<String, Type>,
    pub functions: HashMap<String, Function>,
}
