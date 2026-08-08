use super::*;

pub(in crate::tui) struct ModelFormState {
    pub(in crate::tui) provider_id: String,
    pub(in crate::tui) previous_id: Option<String>,
    pub(in crate::tui) id: String,
    pub(in crate::tui) name: String,
    pub(in crate::tui) api: usize,
    pub(in crate::tui) reasoning: bool,
    pub(in crate::tui) image_input: bool,
    pub(in crate::tui) thinking_off: String,
    pub(in crate::tui) thinking_minimal: String,
    pub(in crate::tui) thinking_low: String,
    pub(in crate::tui) thinking_medium: String,
    pub(in crate::tui) thinking_high: String,
    pub(in crate::tui) thinking_xhigh: String,
    pub(in crate::tui) thinking_max: String,
    pub(in crate::tui) context_window: String,
    pub(in crate::tui) max_tokens: String,
    pub(in crate::tui) input_cost: String,
    pub(in crate::tui) output_cost: String,
    pub(in crate::tui) cache_read_cost: String,
    pub(in crate::tui) cache_write_cost: String,
    pub(in crate::tui) thinking_expanded: bool,
    pub(in crate::tui) limits_expanded: bool,
    pub(in crate::tui) pricing_expanded: bool,
    pub(in crate::tui) field: usize,
    pub(in crate::tui) cursor: usize,
}

impl ModelFormState {
    pub(in crate::tui) fn add(provider_id: &str) -> Self {
        Self {
            provider_id: provider_id.into(),
            previous_id: None,
            id: String::new(),
            name: String::new(),
            api: 0,
            reasoning: false,
            image_input: false,
            thinking_off: String::new(),
            thinking_minimal: String::new(),
            thinking_low: String::new(),
            thinking_medium: String::new(),
            thinking_high: String::new(),
            thinking_xhigh: String::new(),
            thinking_max: String::new(),
            context_window: String::new(),
            max_tokens: String::new(),
            input_cost: String::new(),
            output_cost: String::new(),
            cache_read_cost: String::new(),
            cache_write_cost: String::new(),
            thinking_expanded: false,
            limits_expanded: false,
            pricing_expanded: false,
            field: 0,
            cursor: 0,
        }
    }

    pub(in crate::tui) fn edit(provider_id: &str, model: &ModelView) -> Self {
        let thinking = |level: &str| {
            model
                .thinking_level_map
                .as_ref()
                .and_then(|map| map.get(level))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Self {
            provider_id: provider_id.into(),
            previous_id: Some(model.id.clone()),
            id: model.id.clone(),
            name: model.name.clone().unwrap_or_default(),
            api: model
                .api
                .as_deref()
                .and_then(|api| API_TYPES.iter().position(|candidate| *candidate == api))
                .map(|index| index + 1)
                .unwrap_or_default(),
            reasoning: model.reasoning,
            image_input: model.input.iter().any(|input| input == "image"),
            thinking_off: thinking("off"),
            thinking_minimal: thinking("minimal"),
            thinking_low: thinking("low"),
            thinking_medium: thinking("medium"),
            thinking_high: thinking("high"),
            thinking_xhigh: thinking("xhigh"),
            thinking_max: thinking("max"),
            context_window: model
                .context_window
                .map(|value| value.to_string())
                .unwrap_or_default(),
            max_tokens: model
                .max_tokens
                .map(|value| value.to_string())
                .unwrap_or_default(),
            input_cost: optional_number(model.input_cost),
            output_cost: optional_number(model.output_cost),
            cache_read_cost: optional_number(model.cache_read_cost),
            cache_write_cost: optional_number(model.cache_write_cost),
            thinking_expanded: false,
            limits_expanded: false,
            pricing_expanded: false,
            field: 0,
            cursor: char_len(&model.id),
        }
    }

    pub(in crate::tui) fn copy(provider_id: &str, model: &ModelView) -> Self {
        let mut form = Self::edit(provider_id, model);
        form.id = format!("{}-copy", model.id);
        form.previous_id = None;
        form.cursor = char_len(&form.id);
        form
    }

    pub(in crate::tui) fn visible_fields(&self) -> Vec<usize> {
        let mut fields = vec![0, 1, 2, 3, 4];
        if self.reasoning {
            fields.push(5);
            if self.thinking_expanded {
                fields.extend([6, 7, 8, 9, 10, 11, 12]);
            }
        }
        fields.push(13);
        if self.limits_expanded {
            fields.extend([14, 15]);
        }
        fields.push(16);
        if self.pricing_expanded {
            fields.extend([17, 18, 19, 20]);
        }
        fields
    }

    pub(in crate::tui) fn current_field_id(&self) -> usize {
        self.visible_fields().get(self.field).copied().unwrap_or(0)
    }

    fn field_text(&self, field_id: usize) -> Option<&str> {
        match field_id {
            0 => Some(&self.id),
            1 => Some(&self.name),
            6 => Some(&self.thinking_off),
            7 => Some(&self.thinking_minimal),
            8 => Some(&self.thinking_low),
            9 => Some(&self.thinking_medium),
            10 => Some(&self.thinking_high),
            11 => Some(&self.thinking_xhigh),
            12 => Some(&self.thinking_max),
            14 => Some(&self.context_window),
            15 => Some(&self.max_tokens),
            17 => Some(&self.input_cost),
            18 => Some(&self.output_cost),
            19 => Some(&self.cache_read_cost),
            20 => Some(&self.cache_write_cost),
            _ => None,
        }
    }

    fn field_text_mut(&mut self, field_id: usize) -> Option<&mut String> {
        match field_id {
            0 => Some(&mut self.id),
            1 => Some(&mut self.name),
            6 => Some(&mut self.thinking_off),
            7 => Some(&mut self.thinking_minimal),
            8 => Some(&mut self.thinking_low),
            9 => Some(&mut self.thinking_medium),
            10 => Some(&mut self.thinking_high),
            11 => Some(&mut self.thinking_xhigh),
            12 => Some(&mut self.thinking_max),
            14 => Some(&mut self.context_window),
            15 => Some(&mut self.max_tokens),
            17 => Some(&mut self.input_cost),
            18 => Some(&mut self.output_cost),
            19 => Some(&mut self.cache_read_cost),
            20 => Some(&mut self.cache_write_cost),
            _ => None,
        }
    }

    pub(in crate::tui) fn current_text(&self) -> Option<&str> {
        let field_id = self.current_field_id();
        self.field_text(field_id)
    }

    pub(in crate::tui) fn current_text_mut(&mut self) -> Option<&mut String> {
        let field_id = self.current_field_id();
        self.field_text_mut(field_id)
    }

    pub(in crate::tui) fn current_len(&self) -> usize {
        self.current_text().map(char_len).unwrap_or_default()
    }

    pub(in crate::tui) fn select_field(&mut self, next: usize) {
        let count = self.visible_fields().len();
        if count == 0 {
            self.field = 0;
        } else {
            self.field = next % count;
        }
        self.cursor = self.current_len();
    }

    pub(in crate::tui) fn draft(&self) -> documents::Result<ModelDraft> {
        let mut thinking_level_map = Map::new();
        for (level, value) in [
            ("off", &self.thinking_off),
            ("minimal", &self.thinking_minimal),
            ("low", &self.thinking_low),
            ("medium", &self.thinking_medium),
            ("high", &self.thinking_high),
            ("xhigh", &self.thinking_xhigh),
            ("max", &self.thinking_max),
        ] {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                thinking_level_map.insert(level.into(), Value::String(trimmed.into()));
            }
        }
        Ok(ModelDraft {
            id: self.id.trim().into(),
            name: (!self.name.trim().is_empty()).then(|| self.name.trim().into()),
            api: api_from_index(self.api),
            reasoning: self.reasoning,
            input: if self.image_input {
                vec!["text".into(), "image".into()]
            } else {
                vec!["text".into()]
            },
            context_window: parse_optional_positive_u64(&self.context_window, "context window")?,
            max_tokens: parse_optional_positive_u64(&self.max_tokens, "max tokens")?,
            input_cost: parse_optional_nonnegative_f64(&self.input_cost, "input cost")?,
            output_cost: parse_optional_nonnegative_f64(&self.output_cost, "output cost")?,
            cache_read_cost: parse_optional_nonnegative_f64(
                &self.cache_read_cost,
                "cache read cost",
            )?,
            cache_write_cost: parse_optional_nonnegative_f64(
                &self.cache_write_cost,
                "cache write cost",
            )?,
            thinking_level_map: (!thinking_level_map.is_empty()).then_some(thinking_level_map),
        })
    }
}

pub(in crate::tui) struct ModelDefaultsFormState {
    pub(in crate::tui) context_window: String,
    pub(in crate::tui) max_tokens: String,
    pub(in crate::tui) input_cost: String,
    pub(in crate::tui) output_cost: String,
    pub(in crate::tui) cache_read_cost: String,
    pub(in crate::tui) cache_write_cost: String,
    pub(in crate::tui) field: usize,
    pub(in crate::tui) cursor: usize,
}

impl ModelDefaultsFormState {
    pub(in crate::tui) fn new(defaults: &ModelDefaults) -> Self {
        Self {
            context_window: optional_number(defaults.context_window),
            max_tokens: optional_number(defaults.max_tokens),
            input_cost: optional_number(defaults.input_cost),
            output_cost: optional_number(defaults.output_cost),
            cache_read_cost: optional_number(defaults.cache_read_cost),
            cache_write_cost: optional_number(defaults.cache_write_cost),
            field: 0,
            cursor: 0,
        }
    }

    pub(in crate::tui) fn current_text(&self) -> &str {
        match self.field {
            0 => &self.context_window,
            1 => &self.max_tokens,
            2 => &self.input_cost,
            3 => &self.output_cost,
            4 => &self.cache_read_cost,
            _ => &self.cache_write_cost,
        }
    }

    pub(in crate::tui) fn current_text_mut(&mut self) -> &mut String {
        match self.field {
            0 => &mut self.context_window,
            1 => &mut self.max_tokens,
            2 => &mut self.input_cost,
            3 => &mut self.output_cost,
            4 => &mut self.cache_read_cost,
            _ => &mut self.cache_write_cost,
        }
    }

    pub(in crate::tui) fn select_field(&mut self, next: usize) {
        self.field = next % 6;
        self.cursor = char_len(self.current_text());
    }

    pub(in crate::tui) fn draft(&self) -> documents::Result<ModelDefaults> {
        Ok(ModelDefaults {
            context_window: parse_optional_positive_u64(&self.context_window, "context window")?,
            max_tokens: parse_optional_positive_u64(&self.max_tokens, "max tokens")?,
            input_cost: parse_optional_nonnegative_f64(&self.input_cost, "input cost")?,
            output_cost: parse_optional_nonnegative_f64(&self.output_cost, "output cost")?,
            cache_read_cost: parse_optional_nonnegative_f64(
                &self.cache_read_cost,
                "cache read cost",
            )?,
            cache_write_cost: parse_optional_nonnegative_f64(
                &self.cache_write_cost,
                "cache write cost",
            )?,
        })
    }
}

fn optional_number(value: Option<impl ToString>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn parse_optional_positive_u64(value: &str, field: &str) -> documents::Result<Option<u64>> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    parse_positive_u64(value, field).map(Some)
}

fn parse_optional_nonnegative_f64(value: &str, field: &str) -> documents::Result<Option<f64>> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(Some)
        .ok_or_else(|| AppError::Invalid(format!("{field} must be a non-negative number")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_fields_are_visible_only_for_reasoning_models() {
        let mut form = ModelFormState::add("provider");
        form.thinking_expanded = true;
        assert_eq!(form.visible_fields(), [0, 1, 2, 3, 4, 13, 16]);

        form.reasoning = true;
        assert_eq!(
            form.visible_fields(),
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 16]
        );
    }
}
