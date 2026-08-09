use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize};

macro_rules! define_ordinary_family {
    (
        $ordinary:ident, $full:ident, $values:ident[$length:literal];
        ordinary { $($variant:ident => $wire_name:literal),+ $(,)? }
        extras { $($extra:ident => $extra_wire_name:literal),+ $(,)? }
    ) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[repr(u8)]
        pub enum $ordinary {
            $(#[serde(rename = $wire_name)] $variant),+
        }

        pub const $values: [$ordinary; $length] = [$($ordinary::$variant),+];

        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[repr(u8)]
        pub enum $full {
            $(#[serde(rename = $wire_name)] $variant,)+
            $(#[serde(rename = $extra_wire_name)] $extra),+
        }

        impl $full {
            pub const fn as_ordinary(self) -> Option<$ordinary> {
                if (self as usize) < $values.len() {
                    Some($values[self as usize])
                } else {
                    None
                }
            }

            pub const fn as_str(self) -> &'static str {
                const LABELS: &[&str] = &[$($wire_name),+, $($extra_wire_name),+];
                LABELS[self as usize]
            }
        }

        impl From<$ordinary> for $full {
            fn from(value: $ordinary) -> Self {
                match value { $($ordinary::$variant => Self::$variant),+ }
            }
        }
    };
}

define_ordinary_family!(
    OrdinaryRank, CardRank, ORDINARY_RANKS[13];
    ordinary {
        Two => "2",
        Three => "3",
        Four => "4",
        Five => "5",
        Six => "6",
        Seven => "7",
        Eight => "8",
        Nine => "9",
        Ten => "10",
        Jack => "J",
        Queen => "Q",
        King => "K",
        Ace => "A",
    }
    extras {
        SmallJoker => "small-joker",
        BigJoker => "big-joker",
    }
);

impl OrdinaryRank {
    pub const fn index(self) -> usize {
        self as usize
    }
}

define_ordinary_family!(
    OrdinarySuit, Suit, ORDINARY_SUITS[4];
    ordinary {
        Heart => "heart",
        Diamond => "diamond",
        Club => "club",
        Spade => "spade",
    }
    extras {
        Joker => "joker",
    }
);

macro_rules! bounded_index_type {
    ($name:ident, $upper_bound:literal, $message:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $name(u8);

        impl $name {
            pub const fn new(value: u8) -> Option<Self> {
                if value < $upper_bound {
                    Some(Self(value))
                } else {
                    None
                }
            }

            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = u8::deserialize(deserializer)?;
                Self::new(value).ok_or_else(|| serde::de::Error::custom($message))
            }
        }
    };
}

bounded_index_type!(Seat, 4, "seat must be between 0 and 3");

impl Seat {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1);
    pub const TWO: Self = Self(2);
    pub const THREE: Self = Self(3);

    pub const fn value(self) -> u8 {
        self.0
    }

    pub const fn all() -> [Self; 4] {
        [Self::ZERO, Self::ONE, Self::TWO, Self::THREE]
    }

    pub const fn next(self) -> Self {
        Self((self.0 + 1) % 4)
    }

    pub const fn partner(self) -> Self {
        Self((self.0 + 2) % 4)
    }

    pub const fn team(self) -> Team {
        Team(self.0 % 2)
    }
}

bounded_index_type!(Team, 2, "team must be 0 or 1");

impl Team {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1);
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: String,
    pub deck_index: u8,
    pub suit: Suit,
    pub rank: CardRank,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CombinationKind {
    Single,
    Pair,
    Triple,
    FullHouse,
    Straight,
    ConsecutivePairs,
    ConsecutiveTriples,
    Bomb,
    StraightFlush,
    JokerBomb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WildcardAssignment {
    pub rank: OrdinaryRank,
    pub suit: OrdinarySuit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Combination {
    pub kind: CombinationKind,
    pub size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_rank: Option<CardRank>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_top: Option<OrdinaryRank>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suit: Option<OrdinarySuit>,
    #[serde(default)]
    pub wildcard_assignments: IndexMap<String, WildcardAssignment>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CombinationDeclaration {
    pub kind: CombinationKind,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_rank: Option<CardRank>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_top: Option<OrdinaryRank>,
}

/// Preserves the wire-level distinction between an omitted optional field and
/// an explicit JSON `null`, matching the client schema.
pub(crate) fn deserialize_optional_non_null<'de, D, T>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_values_and_checked_numeric_types_match() {
        assert_eq!(serde_json::to_string(&Seat::THREE).unwrap(), "3");
        assert_eq!(serde_json::from_str::<Seat>("2").unwrap(), Seat::TWO);
        assert!(serde_json::from_str::<Seat>("4").is_err());
        assert_eq!(serde_json::to_string(&Team::ONE).unwrap(), "1");
        assert!(serde_json::from_str::<Team>("2").is_err());
        let heart = serde_json::to_string(&OrdinarySuit::Heart).unwrap();
        assert_eq!(heart, "\"heart\"");
    }

    #[test]
    fn ordinary_rank_indices_and_card_ranks_stay_aligned() {
        for (index, rank) in ORDINARY_RANKS.into_iter().enumerate() {
            assert_eq!(rank.index(), index);
            assert_eq!(CardRank::from(rank).as_ordinary(), Some(rank));
        }
        assert_eq!(CardRank::SmallJoker.as_ordinary(), None);
    }

    #[test]
    fn combination_wire_names_and_optional_fields_match_typescript() {
        let combination = Combination {
            kind: CombinationKind::FullHouse,
            size: 5,
            primary_rank: Some(CardRank::Queen),
            sequence_top: None,
            suit: None,
            wildcard_assignments: IndexMap::new(),
        };
        let value = serde_json::to_value(combination).unwrap();
        assert_eq!(value["kind"], "full-house");
        assert_eq!(value["primaryRank"], "Q");
        assert!(value.get("sequenceTop").is_none());
        assert!(value.get("suit").is_none());
    }
}
