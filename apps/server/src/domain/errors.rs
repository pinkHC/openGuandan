use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct RuleError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl RuleError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub(crate) fn internal(details: impl Into<String>) -> Self {
        Self::new("INTERNAL_ERROR", "服务器内部错误").with_details(Value::String(details.into()))
    }
}
