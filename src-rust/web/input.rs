use serde_json::{Map, Value};

use crate::documents::{ModelDefaults, ModelDraft, ProviderDraft, Result};

use super::invalid;

pub(super) struct Input<'a> {
    value: &'a Map<String, Value>,
    path: String,
}

impl<'a> Input<'a> {
    pub(super) fn new(value: &'a Value, path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        let value = value
            .as_object()
            .ok_or_else(|| invalid(format!("{path} must be an object")))?;
        Ok(Self { value, path })
    }

    pub(super) fn check_fields(&self, fields: &[&str]) -> Result<()> {
        for key in self.value.keys() {
            if !(fields.contains(&key.as_str()) || self.path == "request" && key == "action") {
                return Err(invalid(format!("unknown field {}.{key}", self.path)));
            }
        }
        Ok(())
    }

    fn type_error(&self, field: &str, kind: &str) -> crate::documents::AppError {
        invalid(format!("{}.{field} must be {kind}", self.path))
    }

    fn required(&self, field: &str) -> Result<&'a Value> {
        self.value
            .get(field)
            .ok_or_else(|| invalid(format!("{}.{field} is required", self.path)))
    }

    pub(super) fn object(&self, field: &str) -> Result<Input<'a>> {
        Input::new(self.required(field)?, format!("{}.{field}", self.path))
    }

    pub(super) fn string(&self, field: &str) -> Result<&'a str> {
        self.required(field)?
            .as_str()
            .ok_or_else(|| self.type_error(field, "a string"))
    }

    pub(super) fn boolean(&self, field: &str) -> Result<bool> {
        self.required(field)?
            .as_bool()
            .ok_or_else(|| self.type_error(field, "a boolean"))
    }

    pub(super) fn integer(&self, field: &str) -> Result<u64> {
        self.required(field)?
            .as_u64()
            .ok_or_else(|| self.type_error(field, "a non-negative integer"))
    }

    pub(super) fn strings(&self, field: &str) -> Result<Vec<String>> {
        let items = self
            .required(field)?
            .as_array()
            .ok_or_else(|| self.type_error(field, "an array of strings"))?;
        items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                item.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| self.type_error(&format!("{field}[{index}]"), "a string"))
            })
            .collect()
    }

    pub(super) fn indices(&self, field: &str) -> Result<Vec<usize>> {
        let items = self
            .required(field)?
            .as_array()
            .ok_or_else(|| self.type_error(field, "an array of indices"))?;
        items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                item.as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| {
                        self.type_error(&format!("{field}[{index}]"), "a non-negative integer")
                    })
            })
            .collect()
    }

    fn optional<T>(
        &self,
        field: &str,
        kind: &str,
        parse: impl FnOnce(&Value) -> Option<T>,
    ) -> Result<Option<T>> {
        match self.value.get(field) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => parse(value)
                .map(Some)
                .ok_or_else(|| self.type_error(field, &format!("{kind} or null"))),
        }
    }

    pub(super) fn has(&self, field: &str) -> bool {
        self.value.contains_key(field)
    }

    pub(super) fn optional_string(&self, field: &str) -> Result<Option<String>> {
        self.optional(field, "a string", |value| value.as_str().map(str::to_owned))
    }

    pub(super) fn optional_boolean(&self, field: &str) -> Result<Option<bool>> {
        self.optional(field, "a boolean", Value::as_bool)
    }

    pub(super) fn optional_integer(&self, field: &str) -> Result<Option<u64>> {
        self.optional(field, "a non-negative integer", Value::as_u64)
    }

    fn optional_cost(&self, field: &str) -> Result<Option<f64>> {
        self.optional(field, "a non-negative number", |value| {
            value
                .as_f64()
                .filter(|value| value.is_finite() && *value >= 0.0)
        })
    }

    fn optional_map(&self, field: &str) -> Result<Option<Map<String, Value>>> {
        self.optional(field, "an object", |value| value.as_object().cloned())
    }
}

pub(super) fn provider_draft(input: &Input<'_>) -> Result<ProviderDraft> {
    input.check_fields(&[
        "id",
        "inPi",
        "baseUrl",
        "api",
        "apiKey",
        "authHeader",
        "headers",
        "compat",
    ])?;
    Ok(ProviderDraft {
        id: input.string("id")?.into(),
        in_pi: input.boolean("inPi")?,
        base_url: input.string("baseUrl")?.into(),
        api: input.optional_string("api")?,
        api_key: input.string("apiKey")?.into(),
        auth_header: input.boolean("authHeader")?,
        headers: input.optional_map("headers")?.map(Value::Object),
        compat: input.optional_map("compat")?.map(Value::Object),
    })
}

pub(super) fn model_draft(input: &Input<'_>) -> Result<ModelDraft> {
    input.check_fields(&[
        "id",
        "name",
        "api",
        "reasoning",
        "input",
        "contextWindow",
        "maxTokens",
        "inputCost",
        "outputCost",
        "cacheReadCost",
        "cacheWriteCost",
        "thinkingLevelMap",
    ])?;
    Ok(ModelDraft {
        id: input.string("id")?.into(),
        name: input.optional_string("name")?,
        api: input.optional_string("api")?,
        reasoning: input.boolean("reasoning")?,
        input: input.strings("input")?,
        context_window: input.optional_integer("contextWindow")?,
        max_tokens: input.optional_integer("maxTokens")?,
        input_cost: input.optional_cost("inputCost")?,
        output_cost: input.optional_cost("outputCost")?,
        cache_read_cost: input.optional_cost("cacheReadCost")?,
        cache_write_cost: input.optional_cost("cacheWriteCost")?,
        thinking_level_map: input.optional_map("thinkingLevelMap")?,
    })
}

pub(super) fn model_defaults(input: &Input<'_>) -> Result<ModelDefaults> {
    input.check_fields(&[
        "contextWindow",
        "maxTokens",
        "inputCost",
        "outputCost",
        "cacheReadCost",
        "cacheWriteCost",
    ])?;
    Ok(ModelDefaults {
        context_window: input.optional_integer("contextWindow")?,
        max_tokens: input.optional_integer("maxTokens")?,
        input_cost: input.optional_cost("inputCost")?,
        output_cost: input.optional_cost("outputCost")?,
        cache_read_cost: input.optional_cost("cacheReadCost")?,
        cache_write_cost: input.optional_cost("cacheWriteCost")?,
    })
}
