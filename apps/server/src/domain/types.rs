use std::fmt;

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const ORDINARY_RANKS: [OrdinaryRank; 13] = [
    OrdinaryRank::Two,
    OrdinaryRank::Three,
    OrdinaryRank::Four,
    OrdinaryRank::Five,
    OrdinaryRank::Six,
    OrdinaryRank::Seven,
    OrdinaryRank::Eight,
    OrdinaryRank::Nine,
    OrdinaryRank::Ten,
    OrdinaryRank::Jack,
    OrdinaryRank::Queen,
    OrdinaryRank::King,
    OrdinaryRank::Ace,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OrdinaryRank {
    #[serde(rename = "2")]
    Two,
    #[serde(rename = "3")]
    Three,
    #[serde(rename = "4")]
    Four,
    #[serde(rename = "5")]
    Five,
    #[serde(rename = "6")]
    Six,
    #[serde(rename = "7")]
    Seven,
    #[serde(rename = "8")]
    Eight,
    #[serde(rename = "9")]
    Nine,
    #[serde(rename = "10")]
    Ten,
    #[serde(rename = "J")]
    Jack,
    #[serde(rename = "Q")]
    Queen,
    #[serde(rename = "K")]
    King,
    #[serde(rename = "A")]
    Ace,
}

impl OrdinaryRank {
    pub const fn index(self) -> usize {
        match self {
            Self::Two => 0,
            Self::Three => 1,
            Self::Four => 2,
            Self::Five => 3,
            Self::Six => 4,
            Self::Seven => 5,
            Self::Eight => 6,
            Self::Nine => 7,
            Self::Ten => 8,
            Self::Jack => 9,
            Self::Queen => 10,
            Self::King => 11,
            Self::Ace => 12,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
            Self::Six => "6",
            Self::Seven => "7",
            Self::Eight => "8",
            Self::Nine => "9",
            Self::Ten => "10",
            Self::Jack => "J",
            Self::Queen => "Q",
            Self::King => "K",
            Self::Ace => "A",
        }
    }
}

impl fmt::Display for OrdinaryRank {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CardRank {
    #[serde(rename = "2")]
    Two,
    #[serde(rename = "3")]
    Three,
    #[serde(rename = "4")]
    Four,
    #[serde(rename = "5")]
    Five,
    #[serde(rename = "6")]
    Six,
    #[serde(rename = "7")]
    Seven,
    #[serde(rename = "8")]
    Eight,
    #[serde(rename = "9")]
    Nine,
    #[serde(rename = "10")]
    Ten,
    #[serde(rename = "J")]
    Jack,
    #[serde(rename = "Q")]
    Queen,
    #[serde(rename = "K")]
    King,
    #[serde(rename = "A")]
    Ace,
    #[serde(rename = "small-joker")]
    SmallJoker,
    #[serde(rename = "big-joker")]
    BigJoker,
}

impl CardRank {
    pub const fn as_ordinary(self) -> Option<OrdinaryRank> {
        match self {
            Self::Two => Some(OrdinaryRank::Two),
            Self::Three => Some(OrdinaryRank::Three),
            Self::Four => Some(OrdinaryRank::Four),
            Self::Five => Some(OrdinaryRank::Five),
            Self::Six => Some(OrdinaryRank::Six),
            Self::Seven => Some(OrdinaryRank::Seven),
            Self::Eight => Some(OrdinaryRank::Eight),
            Self::Nine => Some(OrdinaryRank::Nine),
            Self::Ten => Some(OrdinaryRank::Ten),
            Self::Jack => Some(OrdinaryRank::Jack),
            Self::Queen => Some(OrdinaryRank::Queen),
            Self::King => Some(OrdinaryRank::King),
            Self::Ace => Some(OrdinaryRank::Ace),
            Self::SmallJoker | Self::BigJoker => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
            Self::Six => "6",
            Self::Seven => "7",
            Self::Eight => "8",
            Self::Nine => "9",
            Self::Ten => "10",
            Self::Jack => "J",
            Self::Queen => "Q",
            Self::King => "K",
            Self::Ace => "A",
            Self::SmallJoker => "small-joker",
            Self::BigJoker => "big-joker",
        }
    }
}

impl From<OrdinaryRank> for CardRank {
    fn from(rank: OrdinaryRank) -> Self {
        match rank {
            OrdinaryRank::Two => Self::Two,
            OrdinaryRank::Three => Self::Three,
            OrdinaryRank::Four => Self::Four,
            OrdinaryRank::Five => Self::Five,
            OrdinaryRank::Six => Self::Six,
            OrdinaryRank::Seven => Self::Seven,
            OrdinaryRank::Eight => Self::Eight,
            OrdinaryRank::Nine => Self::Nine,
            OrdinaryRank::Ten => Self::Ten,
            OrdinaryRank::Jack => Self::Jack,
            OrdinaryRank::Queen => Self::Queen,
            OrdinaryRank::King => Self::King,
            OrdinaryRank::Ace => Self::Ace,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Suit {
    Heart,
    Diamond,
    Club,
    Spade,
    Joker,
}

impl Suit {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Heart => "heart",
            Self::Diamond => "diamond",
            Self::Club => "club",
            Self::Spade => "spade",
            Self::Joker => "joker",
        }
    }

    pub const fn as_ordinary(self) -> Option<OrdinarySuit> {
        match self {
            Self::Heart => Some(OrdinarySuit::Heart),
            Self::Diamond => Some(OrdinarySuit::Diamond),
            Self::Club => Some(OrdinarySuit::Club),
            Self::Spade => Some(OrdinarySuit::Spade),
            Self::Joker => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrdinarySuit {
    Heart,
    Diamond,
    Club,
    Spade,
}

impl From<OrdinarySuit> for Suit {
    fn from(suit: OrdinarySuit) -> Self {
        match suit {
            OrdinarySuit::Heart => Self::Heart,
            OrdinarySuit::Diamond => Self::Diamond,
            OrdinarySuit::Club => Self::Club,
            OrdinarySuit::Spade => Self::Spade,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Seat(u8);

impl Seat {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1);
    pub const TWO: Self = Self(2);
    pub const THREE: Self = Self(3);

    pub const fn new(value: u8) -> Option<Self> {
        if value < 4 { Some(Self(value)) } else { None }
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }

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

impl Serialize for Seat {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for Seat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| serde::de::Error::custom("seat must be between 0 and 3"))
    }
}

impl From<Seat> for usize {
    fn from(seat: Seat) -> Self {
        seat.index()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Team(u8);

impl Team {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1);

    pub const fn new(value: u8) -> Option<Self> {
        if value < 2 { Some(Self(value)) } else { None }
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub const fn value(self) -> u8 {
        self.0
    }

    pub const fn all() -> [Self; 2] {
        [Self::ZERO, Self::ONE]
    }
}

impl Serialize for Team {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for Team {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| serde::de::Error::custom("team must be 0 or 1"))
    }
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
#[serde(rename_all = "camelCase")]
pub struct CombinationDeclaration {
    pub kind: CombinationKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_rank: Option<CardRank>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_top: Option<OrdinaryRank>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_numeric_seat_and_team_serde() {
        assert_eq!(serde_json::to_string(&Seat::THREE).unwrap(), "3");
        assert_eq!(serde_json::from_str::<Seat>("2").unwrap(), Seat::TWO);
        assert!(serde_json::from_str::<Seat>("4").is_err());
        assert_eq!(serde_json::to_string(&Team::ONE).unwrap(), "1");
        assert!(serde_json::from_str::<Team>("2").is_err());
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
