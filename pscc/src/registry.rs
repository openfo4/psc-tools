use std::collections::HashMap;

use crate::ast::{Function, Type};

#[derive(Clone, Debug, Default)]
pub struct Registry {
    pub classes: HashMap<String, ClassInfo>,
}

#[derive(Clone, Debug, Default)]
pub struct ClassInfo {
    pub properties: HashMap<String, Type>,
    pub functions: HashMap<String, Function>,
}
