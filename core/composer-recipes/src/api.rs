//! The JSON level of `templates.list`, `template.apply`, `recipes.list`, `recipe.plan` and
//! `recipe.apply`, shared by the daemon and the browser editor so both answer alike. Callers
//! add their own transport: errors, the change feed and the session summary.

use crate::error::RecipeError;
use crate::recipes::{self, Scope};
use crate::templates;
use composer_commands::{Command, CommandError, Editor, Source};
use composer_model::Project;
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// Params of `template.apply`: `{template, seed?, dry_run?}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateRequest {
    pub template: String,
    pub seed: Option<u64>,
    #[serde(default)]
    pub dry_run: bool,
}

/// Params of `recipe.plan` and `recipe.apply`: `{recipe, params?, scope?, seed?, dry_run?}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeRequest {
    pub recipe: String,
    #[serde(default)]
    pub params: Value,
    pub scope: Option<Scope>,
    pub seed: Option<u64>,
    #[serde(default)]
    pub dry_run: bool,
}

/// Commands ready to plan or apply as one group, plus fields to echo to the client.
#[derive(Debug, Clone)]
pub struct Planned {
    pub label: String,
    pub source: Source,
    pub commands: Vec<Command>,
    /// Object merged into the reply: what was asked for and what the list contains.
    pub extra: Map<String, Value>,
}

/// A command of the list that the project refused, with its position in the list.
#[derive(Debug, Clone)]
pub struct PlanFailure {
    pub error: CommandError,
    pub index: Option<usize>,
}

pub fn templates_list() -> Value {
    json!({ "templates": templates::all().iter().map(templates::Template::info).collect::<Vec<_>>() })
}

pub fn recipes_list() -> Value {
    json!({ "recipes": recipes::list() })
}

pub fn plan_template(project: &Project, req: &TemplateRequest) -> Result<Planned, RecipeError> {
    let t = templates::get(&req.template).ok_or_else(|| {
        RecipeError::new(
            "unknown_template",
            format!("no template named {}", req.template),
        )
    })?;
    let seed = req.seed.unwrap_or(0);
    let p = templates::plan(project, t, seed)?;
    let mut extra = Map::new();
    extra.insert("template".into(), json!(t.id));
    extra.insert("seed".into(), json!(seed));
    extra.insert("ids".into(), json!(p.ids));
    extra.insert("tempo".into(), json!(t.tempo));
    extra.insert("key".into(), json!(t.key_text));
    extra.insert(
        "loop".into(),
        json!({ "start": t.loop_start, "end": t.loop_end }),
    );
    Ok(Planned {
        label: format!("Template: {}", t.name),
        source: Source::Template,
        commands: p.commands,
        extra,
    })
}

pub fn plan_recipe(project: &Project, req: &RecipeRequest) -> Result<Planned, RecipeError> {
    let seed = req.seed.unwrap_or(0);
    let p = recipes::plan(
        project,
        &req.recipe,
        req.params.clone(),
        req.scope.unwrap_or_default(),
        seed,
    )?;
    let mut extra = Map::new();
    extra.insert("recipe".into(), json!(req.recipe));
    extra.insert("seed".into(), json!(seed));
    Ok(Planned {
        label: p.label,
        source: Source::Recipe,
        commands: p.commands,
        extra,
    })
}

/// Plans every command in order on a copy. Returns the copy and the count of primitive
/// changes; the project itself is not touched.
pub fn check(project: &Project, commands: &[Command]) -> Result<(Project, usize), PlanFailure> {
    let mut copy = project.clone();
    let mut count = 0;
    for (i, c) in commands.iter().enumerate() {
        let at = (commands.len() > 1).then_some(i);
        let changes = c
            .plan(&copy)
            .map_err(|error| PlanFailure { error, index: at })?;
        for ch in &changes {
            copy.apply(ch).map_err(|e| PlanFailure {
                error: CommandError::from(e),
                index: at,
            })?;
        }
        count += changes.len();
    }
    Ok((copy, count))
}

fn hex(h: u64) -> String {
    format!("{h:016x}")
}

/// The reply of `recipe.plan` (and of a dry run): the commands themselves plus how they
/// would change the project, so a client can show them, post them as a proposal, or send
/// them to `command.group`.
pub fn report(project: &Project, planned: &Planned) -> Result<Value, PlanFailure> {
    let (after, changes) = check(project, &planned.commands)?;
    let mut out = planned.extra.clone();
    out.insert("label".into(), json!(planned.label));
    out.insert("command_count".into(), json!(planned.commands.len()));
    out.insert("change_count".into(), json!(changes));
    out.insert("would_change".into(), json!(changes > 0));
    out.insert("applies".into(), json!(true));
    out.insert("state_hash".into(), json!(hex(project.state_hash())));
    out.insert("state_hash_after".into(), json!(hex(after.state_hash())));
    out.insert("commands".into(), json!(planned.commands));
    Ok(Value::Object(out))
}

/// Applies the list as one undo group tagged with the planned source. On failure the group
/// is rolled back and the project is unchanged.
pub fn apply(editor: &mut Editor, planned: &Planned) -> Result<(), PlanFailure> {
    let mut failed = None;
    let run = editor.group_as(&planned.label, planned.source, |ed| {
        for (i, c) in planned.commands.iter().enumerate() {
            if let Err(e) = ed.execute(c) {
                failed = Some(i);
                return Err(e);
            }
        }
        Ok(())
    });
    run.map_err(|error| PlanFailure {
        error,
        index: failed,
    })
}

/// Fields every apply reply carries besides the session summary.
pub fn applied_fields(planned: &Planned) -> Value {
    let mut out = planned.extra.clone();
    out.insert("label".into(), json!(planned.label));
    out.insert("command_count".into(), json!(planned.commands.len()));
    Value::Object(out)
}
