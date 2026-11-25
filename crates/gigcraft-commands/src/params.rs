use crate::CommandError;
use serde_json::{Map, Value};
use std::str::FromStr;

/// Typed access to a command's JSON parameters.
#[derive(Debug)]
pub(crate) struct Params<'a> {
    map: Option<&'a Map<String, Value>>,
}

impl<'a> Params<'a> {
    pub(crate) fn new(value: &'a Value) -> Result<Self, CommandError> {
        match value {
            Value::Null => Ok(Self { map: None }),
            Value::Object(m) => Ok(Self { map: Some(m) }),
            _ => Err(CommandError::InvalidParams(
                "parameters must be a JSON object".into(),
            )),
        }
    }

    pub(crate) fn raw(&self, name: &str) -> Option<&'a Value> {
        self.map.and_then(|m| m.get(name)).filter(|v| !v.is_null())
    }

    fn bad(name: &str, what: &str) -> CommandError {
        CommandError::InvalidParams(format!("`{name}` must be {what}"))
    }

    pub(crate) fn opt_str(&self, name: &str) -> Result<Option<&'a str>, CommandError> {
        match self.raw(name) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.as_str())),
            Some(_) => Err(Self::bad(name, "a string")),
        }
    }

    pub(crate) fn str(&self, name: &str) -> Result<&'a str, CommandError> {
        self.opt_str(name)?
            .ok_or_else(|| CommandError::InvalidParams(format!("`{name}` is required")))
    }

    pub(crate) fn id<T: FromStr>(&self, name: &str) -> Result<T, CommandError> {
        self.str(name)?
            .parse()
            .map_err(|_| Self::bad(name, "an id (UUID)"))
    }

    pub(crate) fn opt_u32(&self, name: &str) -> Result<Option<u32>, CommandError> {
        match self.raw(name) {
            None => Ok(None),
            Some(v) => v
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .map(Some)
                .ok_or_else(|| Self::bad(name, "a non-negative integer")),
        }
    }

    pub(crate) fn opt_usize(&self, name: &str) -> Result<Option<usize>, CommandError> {
        Ok(self.opt_u32(name)?.map(|n| n as usize))
    }

    pub(crate) fn opt_i64(&self, name: &str) -> Result<Option<i64>, CommandError> {
        match self.raw(name) {
            None => Ok(None),
            Some(v) => v
                .as_i64()
                .map(Some)
                .ok_or_else(|| Self::bad(name, "an integer")),
        }
    }

    pub(crate) fn opt_f64(&self, name: &str) -> Result<Option<f64>, CommandError> {
        match self.raw(name) {
            None => Ok(None),
            Some(v) => v
                .as_f64()
                .map(Some)
                .ok_or_else(|| Self::bad(name, "a number")),
        }
    }

    pub(crate) fn opt_bool(&self, name: &str) -> Result<Option<bool>, CommandError> {
        match self.raw(name) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(Self::bad(name, "true or false")),
        }
    }

    pub(crate) fn opt_object(
        &self,
        name: &str,
    ) -> Result<Option<&'a Map<String, Value>>, CommandError> {
        match self.raw(name) {
            None => Ok(None),
            Some(Value::Object(m)) => Ok(Some(m)),
            Some(_) => Err(Self::bad(name, "an object")),
        }
    }
}
