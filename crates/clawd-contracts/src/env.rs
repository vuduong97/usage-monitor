use std::collections::HashMap;
use std::sync::Arc;

/// The process environment as an injectable value (`os.environ` in Python).
///
/// Every function that reads `CLAWD_*` (or `CODEX_HOME`, ...) takes an `&Env` instead of calling
/// `std::env::var`, so tests running in parallel never race on the real environment (red team
/// #14). Production passes [`Env::system`]; tests pass [`Env::from_map`].
#[derive(Clone)]
pub struct Env(Arc<EnvFn>);

type EnvFn = dyn Fn(&str) -> Option<String> + Send + Sync;

impl Env {
    /// The real process environment (empty values count as unset, like Python's `or`).
    pub fn system() -> Self {
        Self(Arc::new(|name| {
            std::env::var(name).ok().filter(|v| !v.is_empty())
        }))
    }

    /// A fixed set of variables.
    pub fn from_map(vars: HashMap<String, String>) -> Self {
        Self(Arc::new(move |name| {
            vars.get(name).cloned().filter(|v| !v.is_empty())
        }))
    }

    /// No variables at all.
    pub fn empty() -> Self {
        Self::from_map(HashMap::new())
    }

    /// Any lookup function.
    pub fn from_fn(f: impl Fn(&str) -> Option<String> + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    /// `os.environ.get(name)`.
    pub fn get(&self, name: &str) -> Option<String> {
        (self.0)(name)
    }
}

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Env(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_env_is_isolated_and_empty_means_unset() {
        let env = Env::from_map(HashMap::from([
            ("CLAWD_EVENT_PORT".to_owned(), "0".to_owned()),
            ("BLANK".to_owned(), String::new()),
        ]));
        assert_eq!(env.get("CLAWD_EVENT_PORT").as_deref(), Some("0"));
        assert_eq!(env.get("BLANK"), None);
        assert_eq!(Env::empty().get("PATH"), None);
    }
}
