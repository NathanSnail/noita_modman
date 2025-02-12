use crate::GitHost;
use crate::GitMod;
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

impl ConditionalVariant {
    fn new(pat: &str) -> Option<ConditionalVariant> {
        // should probably make this better
        if pat.starts_with("e") && "enabled".starts_with(pat) {
            Some(ConditionalVariant::Enabled)
        } else if pat.starts_with("ga") && "gamemode".starts_with(pat) {
            Some(ConditionalVariant::Gamemode)
        } else if pat.starts_with("gi") && "git".starts_with(pat) {
            Some(ConditionalVariant::Git)
        } else if pat.starts_with("gith") && "github".starts_with(pat) {
            Some(ConditionalVariant::Github)
        } else if pat.starts_with("gitl") && "gitlab".starts_with(pat) {
            Some(ConditionalVariant::Gitlab)
        } else if pat.starts_with("m") && "manual".starts_with(pat) {
            Some(ConditionalVariant::Manual)
        } else if pat.starts_with("n") && "normal".starts_with(pat) {
            Some(ConditionalVariant::Normal)
        } else if pat.starts_with("st") && "steam".starts_with(pat) {
            Some(ConditionalVariant::Steam)
        } else if pat.starts_with("sa") && "safe".starts_with(pat) {
            Some(ConditionalVariant::Safe)
        } else if pat.starts_with("t") && "translation".starts_with(pat) {
            Some(ConditionalVariant::Translation)
        } else {
            None
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
                    None
                }
            }
            ConditionalVariant::Gitlab => {
                if let ModSource::Git(source) = &nmod.source {
                    Some(matches!(source.host, GitHost::Gitlab))
                } else {
                    None
                }
            }
            ConditionalVariant::Manual => Some(matches!(nmod.source, ModSource::Manual)),
            ConditionalVariant::Normal => Some(matches!(nmod.kind, ModKind::Gamemode)),
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
    pub fn special_terms() -> &'static str {
        "Special terms (use with # or #!): enabled\ngamemode\ngit\ngithub\ngitlab\nmanual\nnormal\nsteam\nsafe\ntranslation"
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
