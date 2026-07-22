use crate::documents::{self, ModelDraft, ModelView, ProviderDraft, ProviderView};

use super::{
    input::{api_from_index, char_len, parse_optional_object, parse_positive_u64},
    API_TYPES,
};

pub(super) struct FormState {
    pub(super) previous_id: Option<String>,
    pub(super) id: String,
    pub(super) base_url: String,
    pub(super) api: usize,
    pub(super) api_key: String,
    pub(super) auth_header: bool,
    pub(super) headers_json: String,
    pub(super) compat_json: String,
    pub(super) field: usize,
    pub(super) cursor: usize,
}

impl FormState {
    pub(super) fn add() -> Self {
        Self {
            previous_id: None,
            id: String::new(),
            base_url: "https://api.openai.com/v1".into(),
            api: 1,
            api_key: "$OPENAI_API_KEY".into(),
            auth_header: true,
            headers_json: String::new(),
            compat_json: String::new(),
            field: 0,
            cursor: 0,
        }
    }

    pub(super) fn edit(provider: &ProviderView) -> Self {
        let mut form = Self {
            previous_id: Some(provider.id.clone()),
            id: provider.id.clone(),
            base_url: provider.base_url.clone(),
            api: API_TYPES
                .iter()
                .position(|api| *api == provider.api)
                .map(|index| index + 1)
                .unwrap_or_default(),
            api_key: provider.api_key.clone(),
            auth_header: provider.auth_header,
            headers_json: provider
                .raw
                .get("headers")
                .map(ToString::to_string)
                .unwrap_or_default(),
            compat_json: provider
                .raw
                .get("compat")
                .map(ToString::to_string)
                .unwrap_or_default(),
            field: 0,
            cursor: 0,
        };
        form.cursor = form.current_len();
        form
    }

    pub(super) fn current_len(&self) -> usize {
        self.current_text().map(char_len).unwrap_or_default()
    }

    pub(super) fn current_text(&self) -> Option<&str> {
        match self.field {
            0 => Some(&self.id),
            1 => Some(&self.base_url),
            3 => Some(&self.api_key),
            5 => Some(&self.headers_json),
            6 => Some(&self.compat_json),
            _ => None,
        }
    }

    pub(super) fn current_text_mut(&mut self) -> Option<&mut String> {
        match self.field {
            0 => Some(&mut self.id),
            1 => Some(&mut self.base_url),
            3 => Some(&mut self.api_key),
            5 => Some(&mut self.headers_json),
            6 => Some(&mut self.compat_json),
            _ => None,
        }
    }

    pub(super) fn select_field(&mut self, next: usize) {
        self.field = next % 7;
        self.cursor = self.current_len();
    }

    pub(super) fn draft(&self) -> documents::Result<ProviderDraft> {
        Ok(ProviderDraft {
            id: self.id.trim().into(),
            base_url: self.base_url.trim().into(),
            api: api_from_index(self.api),
            api_key: self.api_key.trim().into(),
            auth_header: self.auth_header,
            headers: parse_optional_object(&self.headers_json, "headers")?,
            compat: parse_optional_object(&self.compat_json, "compat")?,
        })
    }
}

pub(super) struct ModelFormState {
    pub(super) provider_id: String,
    pub(super) previous_id: Option<String>,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) api: usize,
    pub(super) reasoning: bool,
    pub(super) image_input: bool,
    pub(super) context_window: String,
    pub(super) max_tokens: String,
    pub(super) field: usize,
    pub(super) cursor: usize,
}

impl ModelFormState {
    pub(super) fn add(provider_id: &str) -> Self {
        Self {
            provider_id: provider_id.into(),
            previous_id: None,
            id: String::new(),
            name: String::new(),
            api: 0,
            reasoning: false,
            image_input: false,
            context_window: "128000".into(),
            max_tokens: "16384".into(),
            field: 0,
            cursor: 0,
        }
    }

    pub(super) fn edit(provider_id: &str, model: &ModelView) -> Self {
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
            context_window: model.context_window.to_string(),
            max_tokens: model.max_tokens.to_string(),
            field: 0,
            cursor: char_len(&model.id),
        }
    }

    pub(super) fn copy(provider_id: &str, model: &ModelView) -> Self {
        let mut form = Self::edit(provider_id, model);
        form.id = format!("{}-copy", model.id);
        form.previous_id = None;
        form.cursor = char_len(&form.id);
        form
    }

    pub(super) fn current_text(&self) -> Option<&str> {
        match self.field {
            0 => Some(&self.id),
            1 => Some(&self.name),
            5 => Some(&self.context_window),
            6 => Some(&self.max_tokens),
            _ => None,
        }
    }

    pub(super) fn current_text_mut(&mut self) -> Option<&mut String> {
        match self.field {
            0 => Some(&mut self.id),
            1 => Some(&mut self.name),
            5 => Some(&mut self.context_window),
            6 => Some(&mut self.max_tokens),
            _ => None,
        }
    }

    pub(super) fn current_len(&self) -> usize {
        self.current_text().map(char_len).unwrap_or_default()
    }

    pub(super) fn select_field(&mut self, next: usize) {
        self.field = next % 7;
        self.cursor = self.current_len();
    }

    pub(super) fn draft(&self) -> documents::Result<ModelDraft> {
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
            context_window: parse_positive_u64(&self.context_window, "context window")?,
            max_tokens: parse_positive_u64(&self.max_tokens, "max tokens")?,
        })
    }
}
