//! Templates and recipes (D28). Templates are bundled command data; recipes are
//! deterministic generators. Both produce ordinary commands, so every result replays,
//! undoes as one group and can be posted as a proposal. Headless: no UI types, no I/O.

pub mod api;
mod error;
pub mod recipes;
mod rng;
pub mod templates;

pub use error::RecipeError;
pub use recipes::{RecipePlan, Scope};
pub use rng::{IdSource, Rng};
