use std::collections::HashMap;

use crate::ast::{Function, Type};

#[derive(Clone, Debug, Default)]
pub struct Registry {
    pub classes: HashMap<String, ClassInfo>,
    pub structs: HashMap<String, StructInfo>,
}

impl Registry {
    pub fn func(&self, class: &str, func: &str) -> Option<&Function> {
        self.classes
            .get(class)
            .and_then(|class| class.functions.get(func))
    }

    pub fn global(&self, class: &str, global: &str) -> Option<&Type> {
        self.classes
            .get(class)
            .and_then(|class| class.globals.get(global))
    }

    pub fn property(&self, class: &str, prop: &str) -> Option<&Type> {
        self.classes
            .get(class)
            .and_then(|class| class.properties.get(prop))
    }

    pub fn struct_member(&self, struct_: &str, member: &str) -> Option<&Type> {
        self.structs
            .get(struct_)
            .and_then(|st| st.members.get(member))
    }
}

#[derive(Clone, Debug, Default)]
pub struct ClassInfo {
    pub globals: HashMap<String, Type>,
    pub properties: HashMap<String, Type>,
    pub functions: HashMap<String, Function>,
}

#[derive(Clone, Debug)]
pub struct StructInfo {
    pub members: HashMap<String, Type>,
}
