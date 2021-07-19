use std::fmt;

use crate::{Project, Section};

#[derive(Clone, Debug, PartialEq)]
pub enum ReactionType {
    Approval,
    Section(Option<Section>),
    Project(Option<Project>),
    None,
}

impl fmt::Display for ReactionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReactionType::Approval => write!(f, "approval"),
            ReactionType::Section(_) => write!(f, "section"),
            ReactionType::Project(_) => write!(f, "project"),
            ReactionType::None => write!(f, "NONE"),
        }
    }
}