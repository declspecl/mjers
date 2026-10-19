//! Tiles serialize as their notation (`"5m"`, `"1F"`, `"123m55p"`) so traces
//! and reports stay readable.

use std::fmt::Display;
use std::str::FromStr;

use serde::de::Error;
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};

use crate::counts::TileCounts;
use crate::tile::{
    Bonus,
    Tile,
};

fn serialize_display<T: Display, S: Serializer>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

fn deserialize_from_str<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStr,
    T::Err: Display,
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)?.parse().map_err(D::Error::custom)
}

macro_rules! notation_serde {
    ($($ty:ty),*) => {$(
        impl Serialize for $ty {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serialize_display(self, serializer)
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                deserialize_from_str(deserializer)
            }
        }
    )*};
}

notation_serde!(Tile, Bonus, TileCounts);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_as_notation() {
        let tile: Tile = "7z".parse().unwrap();
        assert_eq!(serde_json::to_string(&tile).unwrap(), "\"7z\"");
        assert_eq!(serde_json::from_str::<Tile>("\"7z\"").unwrap(), tile);
        let bonus = Bonus::Season(3);
        assert_eq!(serde_json::to_string(&bonus).unwrap(), "\"3S\"");
        assert_eq!(serde_json::from_str::<Bonus>("\"3S\"").unwrap(), bonus);
        let counts: TileCounts = "123m55p".parse().unwrap();
        assert_eq!(serde_json::from_str::<TileCounts>(&serde_json::to_string(&counts).unwrap()).unwrap(), counts);
    }

    #[test]
    fn rejects_bad_notation() {
        assert!(serde_json::from_str::<Tile>("\"0m\"").is_err());
        assert!(serde_json::from_str::<Bonus>("\"5F\"").is_err());
    }
}
