use crate::GitHost;
use crate::Mod;
use crate::ModKind;
use crate::ModSource;

#[derive(Copy, Clone, Debug)]
enum ConditionalVariant {
    Enabled,
    Gamemode,
    Git,
    Github,
    Gitlab,
    Manual,
    Normal,
    Steam,
    Safe,
    Translation,
}

const CONDITIONS: [(&str, ConditionalVariant); 10] = [
    ("enabled", ConditionalVariant::Enabled),
    ("gamemode", ConditionalVariant::Gamemode),
    ("git", ConditionalVariant::Git),
    ("github", ConditionalVariant::Github),
    ("gitlab", ConditionalVariant::Gitlab),
    ("manual", ConditionalVariant::Manual),
    ("normal", ConditionalVariant::Normal),
    ("steam", ConditionalVariant::Steam),
    ("safe", ConditionalVariant::Safe),
    ("translation", ConditionalVariant::Translation),
];

impl ConditionalVariant {
    fn new(pat: &str) -> Option<ConditionalVariant> {
        let matching: Vec<_> = CONDITIONS.iter().filter(|e| e.0.starts_with(pat)).collect();
        if matching.len() == 1 {
            Some(matching[0].1)
        } else {
            // git prefixes github and gitlab, so it isn't searchable normally
            let starters: Vec<_> = matching
                .iter()
                .filter(|e| {
                    matching
                        .iter()
                        .fold(true, |acc, x| acc && x.0.starts_with(e.0))
                })
                .collect();
            if starters.len() == 1 {
                Some(starters[0].1)
            } else {
                None
            }
        }
    }

    fn matches(&self, nmod: &Mod) -> Option<bool> {
        match &self {
            ConditionalVariant::Enabled => {
                if let ModKind::Normal(normal_mod) = &nmod.kind {
                    Some(normal_mod.enabled)
                } else {
                    None
                }
            }
            ConditionalVariant::Gamemode => Some(matches!(nmod.kind, ModKind::Gamemode)),
            ConditionalVariant::Git => Some(matches!(nmod.source, ModSource::Git(..))),
            ConditionalVariant::Github => {
                if let ModSource::Git(source) = &nmod.source {
                    Some(matches!(source.host, GitHost::Github))
                } else {
                    Some(false)
                }
            }
            ConditionalVariant::Gitlab => {
                if let ModSource::Git(source) = &nmod.source {
                    Some(matches!(source.host, GitHost::Gitlab))
                } else {
                    Some(false)
                }
            }
            ConditionalVariant::Manual => Some(matches!(nmod.source, ModSource::Manual)),
            ConditionalVariant::Normal => Some(matches!(nmod.kind, ModKind::Normal(..))),
            ConditionalVariant::Steam => Some(matches!(nmod.source, ModSource::Steam(..))),
            ConditionalVariant::Safe => Some(!nmod.unsafe_api),
            ConditionalVariant::Translation => Some(matches!(nmod.kind, ModKind::Translation)),
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct Conditional {
    conditional: ConditionalVariant,
    inverted: bool,
}

impl Conditional {
    fn matches(&self, nmod: &Mod) -> bool {
        self.conditional
            .matches(nmod)
            .map(|x| x ^ self.inverted)
            .unwrap_or(true)
    }

    fn new(src: &str) -> Option<Conditional> {
        let inverted = src.chars().nth(0) == Some('!');
        ConditionalVariant::new(&src[(inverted as usize)..]).map(|x| Conditional {
            conditional: x,
            inverted,
        })
    }
}

#[derive(Clone, Debug)]
enum ConditionEnum {
    Conditional(Conditional),
    Literal(String),
}

#[derive(Clone, Debug)]
pub struct Condition(ConditionEnum);

impl Condition {
    pub fn special_terms() -> String {
        let s = "Special terms (use with # or #!):\n".to_owned();
        CONDITIONS.iter().fold(s, |acc, e| acc + "\n" + e.0)
    }

    pub fn new(src: &str) -> Option<Condition> {
        match src.chars().nth(0) {
            Some(c) => {
                if c == '#' {
                    Conditional::new(&src[1..].to_lowercase())
                        .map(|x| Condition(ConditionEnum::Conditional(x)))
                } else {
                    Some(Condition(ConditionEnum::Literal(src.to_lowercase())))
                }
            }
            None => None,
        }
    }

    pub fn matches(&self, nmod: &Mod) -> bool {
        match &self.0 {
            ConditionEnum::Conditional(conditional) => conditional.matches(nmod),
            ConditionEnum::Literal(s) => {
                nmod.name.to_lowercase().contains(s) || nmod.id.to_lowercase().contains(s)
            }
        }
    }
}
