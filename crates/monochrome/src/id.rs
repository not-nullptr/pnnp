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
        )*
    };
}

id![TrackId];
