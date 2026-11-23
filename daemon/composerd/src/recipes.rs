//! D28: `templates.list`, `template.apply`, `recipes.list`, `recipe.plan` and `recipe.apply`.
//! The music and the shapes live in composer-recipes; this file only maps its errors onto
//! the wire and applies the planned list as one group.

use crate::hub::Inner;
use crate::rpc::{parse_params, summary, with, RpcError};
use composer_recipes::api::{self, PlanFailure, Planned, RecipeRequest, TemplateRequest};
use composer_recipes::RecipeError;
use serde_json::{json, Value};

fn recipe_error(e: RecipeError) -> RpcError {
    if e.is_invalid_params() {
        RpcError::invalid_params(e.message)
    } else {
        RpcError::app(e.code, e.message)
    }
}

fn failure(f: PlanFailure) -> RpcError {
    RpcError::command(&f.error, f.index)
}

pub(crate) fn templates_list(_g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let _: Empty = parse_params(params)?;
    Ok(api::templates_list())
}

pub(crate) fn recipes_list(_g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let _: Empty = parse_params(params)?;
    Ok(api::recipes_list())
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

/// Plans, then either reports (dry run) or applies as one group.
fn run(g: &mut Inner, planned: Planned, dry_run: bool) -> Result<Value, RpcError> {
    if dry_run {
        let mut r = api::report(g.editor.project(), &planned).map_err(failure)?;
        r["dry_run"] = json!(true);
        return Ok(r);
    }
    api::apply(&mut g.editor, &planned).map_err(failure)?;
    Ok(with(summary(&g.editor), api::applied_fields(&planned)))
}

pub(crate) fn template_apply(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let req: TemplateRequest = parse_params(params)?;
    let planned = api::plan_template(g.editor.project(), &req).map_err(recipe_error)?;
    run(g, planned, req.dry_run)
}

pub(crate) fn recipe_plan(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let req: RecipeRequest = parse_params(params)?;
    let planned = api::plan_recipe(g.editor.project(), &req).map_err(recipe_error)?;
    api::report(g.editor.project(), &planned).map_err(failure)
}

pub(crate) fn recipe_apply(g: &mut Inner, params: Value) -> Result<Value, RpcError> {
    let req: RecipeRequest = parse_params(params)?;
    let planned = api::plan_recipe(g.editor.project(), &req).map_err(recipe_error)?;
    run(g, planned, req.dry_run)
}
