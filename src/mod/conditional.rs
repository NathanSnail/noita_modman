use crate::r#mod::GitHost;
use crate::r#mod::ModKind;
use crate::r#mod::ModSource;
use crate::Mod;

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
    Tagged,
    Translation,
}

const CONDITIONS: [(&str, ConditionalVariant); 11] = [
    ("enabled", ConditionalVariant::Enabled),
    ("gamemode", ConditionalVariant::Gamemode),
    ("git", ConditionalVariant::Git),
    ("github", ConditionalVariant::Github),
    ("gitlab", ConditionalVariant::Gitlab),
    ("manual", ConditionalVariant::Manual),
    ("normal", ConditionalVariant::Normal),
    ("steam", ConditionalVariant::Steam),
    ("safe", ConditionalVariant::Safe),
    ("tagged", ConditionalVariant::Tagged),
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
            ConditionalVariant::Tagged => Some(nmod.tags.is_some()),
            ConditionalVariant::Translation => Some(matches!(nmod.kind, ModKind::Translation)),
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct MetaCondition {
    conditional: ConditionalVariant,
    inverted: bool,
}

impl MetaCondition {
    fn matches(&self, nmod: &Mod) -> bool {
        self.conditional
            .matches(nmod)
            .map(|x| x ^ self.inverted)
            .unwrap_or(true)
    }

    fn new(src: &str) -> Option<MetaCondition> {
        let inverted = src.chars().nth(0) == Some('!');
        ConditionalVariant::new(&src[(inverted as usize)..]).map(|x| MetaCondition {
            conditional: x,
            inverted,
        })
    }
}

#[derive(Clone, Debug)]
struct TagCondition {
    inverted: bool,
    tag: String,
}

impl TagCondition {
    fn new(src: &str) -> Option<TagCondition> {
        let mut inverted = false;
        let mut src = src;
        if src.chars().nth(0) == Some('!') {
            src = &src[1..];
            inverted = true;
        }
        if src == "" {
            return None;
        }
        Some(TagCondition {
            inverted,
            tag: src.to_owned(),
        })
    }

    fn matches(&self, nmod: &Mod) -> bool {
        // TODO: maybe we should have aliases? quality of life can't be searched due to spaces, but quality works well enough
        if let Some(tags) = &nmod.tags {
            tags.iter()
                .map(|e| e.starts_with(&self.tag))
                .fold(false, |acc, e| acc || e)
                ^ self.inverted
        } else {
            true // NOTE: we have the #tagged option to allow for customising this
        }
    }
}

#[derive(Clone, Debug)]
enum ConditionEnum {
    Meta(MetaCondition),
    Literal(String),
    Tag(TagCondition),
}

#[derive(Clone, Debug)]
pub struct Condition(ConditionEnum);

impl Condition {
    pub fn special_terms() -> String {
        let s =
            "Use :tag or :!tag to search mod tags\nSpecial terms (use with # or #!):\n".to_owned();
        CONDITIONS.iter().fold(s, |acc, e| acc + "\n" + e.0)
    }

    pub fn new(src: &str) -> Option<Condition> {
        match src.chars().nth(0) {
            Some(c) => {
                if c == '#' {
                    MetaCondition::new(&src[1..].to_lowercase())
                        .map(|x| Condition(ConditionEnum::Meta(x)))
                } else if c == ':' {
                    TagCondition::new(&src[1..].to_lowercase())
                        .map(|x| Condition(ConditionEnum::Tag(x)))
                } else {
                    Some(Condition(ConditionEnum::Literal(src.to_lowercase())))
                }
            }
            None => None,
        }
    }

    pub fn matches(&self, nmod: &Mod) -> bool {
        match &self.0 {
            ConditionEnum::Meta(meta) => meta.matches(nmod),
            ConditionEnum::Literal(s) => {
                nmod.name.to_lowercase().contains(s) || nmod.id.to_lowercase().contains(s)
            }
            ConditionEnum::Tag(tag) => tag.matches(nmod),
        }
    }
}
