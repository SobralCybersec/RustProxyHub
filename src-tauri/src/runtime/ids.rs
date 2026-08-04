/* Domain id newtypes. Everything here is a String or i64 underneath; distinct
wrappers stop a session id, model id, or account id from being transposed at a
call site the compiler would otherwise wave through. ProviderName is not here:
it already exists as an enum in the hub (the id with real variant-mixing risk). */

use serde::{Deserialize, Serialize};
use std::fmt;

/* String-backed ids share one shape; the macro keeps the three of them identical
instead of hand-copied impl blocks that drift. */
macro_rules! string_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }
    };
}

string_id!(ModelId);
string_id!(SessionId);
string_id!(AccountId);

/* Upstream's per-turn parent pointer; i64 to match the wire type verbatim. */
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParentMessageId(pub i64);

impl ParentMessageId {
    pub fn get(self) -> i64 {
        self.0
    }
}

impl From<i64> for ParentMessageId {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl fmt::Display for ParentMessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_id_roundtrips_through_serde_transparently() {
        let id = SessionId::new("sess-1");
        let json = serde_json::to_string(&id).unwrap();
        /* transparent: serializes as the bare string, not {"0":"sess-1"} */
        assert_eq!(json, "\"sess-1\"");
        assert_eq!(serde_json::from_str::<SessionId>(&json).unwrap(), id);
    }

    #[test]
    fn parent_message_id_preserves_wire_int() {
        let id = ParentMessageId::from(42);
        assert_eq!(id.get(), 42);
        assert_eq!(serde_json::to_string(&id).unwrap(), "42");
    }
}
