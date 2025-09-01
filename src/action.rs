use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize)]
#[allow(dead_code)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Suspend,
    Resume,
    Quit,
    ClearScreen,
    Error(String),
    Help,

    // Tab management
    NextTab,
    PrevTab,

    // Selection management
    SelectNext,
    SelectPrev,
    Continue(Option<usize>),
}

// HACK: should probably make this nicer

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>, {
        struct ActionVisitor;

        impl<'de> Visitor<'de> for ActionVisitor {
            type Value = Action;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "an Action enum variant")
            }

            fn visit_str<E>(self, v: &str) -> Result<Action, E>
            where
                E: de::Error, {
                if v == "Continue" {
                    Ok(Action::Continue(None))
                } else {
                    // fallback: let serde handle all other strings
                    #[derive(Deserialize)]
                    enum Helper {
                        Tick,
                        Render,
                        Suspend,
                        Resume,
                        Quit,
                        ClearScreen,
                        Help,
                        NextTab,
                        PrevTab,
                        SelectNext,
                        SelectPrev,
                    }

                    let helper: Helper = serde_json::from_str(&format!("\"{v}\""))
                        .map_err(|_| de::Error::unknown_variant(v, &["Continue"]))?;
                    Ok(match helper {
                        Helper::Tick => Action::Tick,
                        Helper::Render => Action::Render,
                        Helper::Suspend => Action::Suspend,
                        Helper::Resume => Action::Resume,
                        Helper::Quit => Action::Quit,
                        Helper::ClearScreen => Action::ClearScreen,
                        Helper::Help => Action::Help,
                        Helper::NextTab => Action::NextTab,
                        Helper::PrevTab => Action::PrevTab,
                        Helper::SelectNext => Action::SelectNext,
                        Helper::SelectPrev => Action::SelectPrev,
                    })
                }
            }
        }

        deserializer.deserialize_any(ActionVisitor)
    }
}
