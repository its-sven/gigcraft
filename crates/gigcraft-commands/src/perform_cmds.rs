use crate::params::Params;
use crate::{CommandError, Engine};
use gigcraft_model::SetlistId;
use gigcraft_perform::{Action, Effect, Item, Show};
use serde_json::{Value, json};

pub(crate) fn run(e: &mut Engine, id: &str, p: &Params<'_>) -> Result<Value, CommandError> {
    match id {
        "perform.start" => {
            let sid: SetlistId = p.id("setlist")?;
            let list = e
                .lib
                .setlist(sid)
                .ok_or_else(|| CommandError::NotFound(format!("no setlist with id {sid}")))?;
            let mut show = Show::new(list, |song| e.lib.song(song));
            let effects = match p.opt_usize("index")? {
                Some(i) => show.goto(i, p.opt_usize("part")?.unwrap_or(0)),
                None => show.start(),
            };
            e.show = Some(show);
            Ok(report(e, &effects))
        }
        "perform.action" => {
            let name = p.str("action")?;
            let action: Action =
                serde_json::from_value(Value::String(name.to_string())).map_err(|_| {
                    let all: Vec<String> = Action::ALL
                        .iter()
                        .filter_map(|a| serde_json::to_value(a).ok())
                        .map(|v| v.to_string())
                        .collect();
                    CommandError::InvalidParams(format!(
                        "unknown action `{name}`; one of {}",
                        all.join(", ")
                    ))
                })?;
            let show = running(e)?;
            let effects = show.apply(action);
            if action == Action::Exit {
                let out = report(e, &effects);
                e.show = None;
                return Ok(out);
            }
            Ok(report(e, &effects))
        }
        "perform.goto" => {
            let index = p.opt_usize("index")?.unwrap_or(0);
            let part = p.opt_usize("part")?.unwrap_or(0);
            let effects = running(e)?.goto(index, part);
            Ok(report(e, &effects))
        }
        "perform.state" => Ok(report(e, &[])),
        "perform.stop" => {
            e.show = None;
            Ok(json!({ "running": false }))
        }
        _ => Err(CommandError::UnknownCommand(id.to_string())),
    }
}

fn running(e: &mut Engine) -> Result<&mut Show, CommandError> {
    e.show
        .as_mut()
        .ok_or_else(|| CommandError::Rejected("perform mode is not running (perform.start)".into()))
}

/// State plus effects; `send_part` effects carry the part's MIDI actions so a
/// client can send them without another lookup.
fn report(e: &Engine, effects: &[Effect]) -> Value {
    let Some(show) = &e.show else {
        return json!({ "running": false, "effects": effects });
    };
    let effects: Vec<Value> = effects
        .iter()
        .map(|eff| {
            let mut v = serde_json::to_value(eff).unwrap_or(Value::Null);
            if let Effect::SendPart { song, part } = eff {
                let part = e.lib.song(*song).and_then(|s| s.parts.get(*part));
                v["part_name"] = json!(part.map(|p| p.name.clone()));
                v["actions"] =
                    serde_json::to_value(part.map(|p| p.actions.clone()).unwrap_or_default())
                        .unwrap_or(Value::Null);
            }
            v
        })
        .collect();
    let current = show
        .current()
        .map(|i| serde_json::to_value(i).unwrap_or(Value::Null));
    let next = show.up_next().map(|i| match i {
        Item::Song { title, key, .. } => json!({ "type": "song", "title": title, "key": key }),
        Item::Break { label, minutes, .. } => {
            json!({ "type": "break", "label": label, "minutes": minutes })
        }
        Item::Missing { .. } => json!({ "type": "missing" }),
    });
    json!({
        "running": true,
        "setlist": show.setlist.to_string(),
        "name": show.name,
        "index": show.index,
        "part": show.part,
        "count": show.items.len(),
        "current": current,
        "next": next,
        "effects": effects,
    })
}
