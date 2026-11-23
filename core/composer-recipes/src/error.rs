//! One error type for template and recipe planning. The daemon and the browser editor map
//! `code` onto their own wire errors, so both report the same reasons.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeError {
    /// Stable reason: `invalid_params`, `unknown_template`, `unknown_recipe`, `no_target`,
    /// `no_key` or `nothing_to_do`.
    pub code: &'static str,
    pub message: String,
}

impl RecipeError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_params", message)
    }

    /// True for a request written badly, as opposed to one this project cannot take.
    pub fn is_invalid_params(&self) -> bool {
        self.code == "invalid_params"
    }
}

impl fmt::Display for RecipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RecipeError {}
