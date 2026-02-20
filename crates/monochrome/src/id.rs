use serde::{Deserialize, Serialize};

macro_rules! id {
    ($($id:ident),*$(,)?) => {
        $(
            #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
            #[serde(transparent)]
            #[repr(transparent)]
            pub struct $id(u64);

            impl From<u64> for $id {
                fn from(value: u64) -> Self {
                    Self(value)
                }
            }

            impl ::std::fmt::Display for $id {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    self.0.fmt(f)
                }
            }
        )*
    };
}

id![TrackId, AlbumId, ArtistId];
