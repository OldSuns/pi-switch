use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};

use super::{
    schema::provider_view,
    snapshot::{lock_provider_documents, write_provider_changes},
    storage::providers_object,
    AppError, Paths, Result,
};

const FIELD: &str = "ordering";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProfileSort {
    #[default]
    Custom,
    NameAsc,
    NameDesc,
    AddedAsc,
    AddedDesc,
}

impl ProfileSort {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "custom" => Ok(Self::Custom),
            "name-asc" => Ok(Self::NameAsc),
            "name-desc" => Ok(Self::NameDesc),
            "added-asc" => Ok(Self::AddedAsc),
            "added-desc" => Ok(Self::AddedDesc),
            _ => Err(invalid(format!("unsupported sort mode '{value}'"))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::NameAsc => "name-asc",
            Self::NameDesc => "name-desc",
            Self::AddedAsc => "added-asc",
            Self::AddedDesc => "added-desc",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ListOrdering {
    pub sort: ProfileSort,
    pub order: Vec<String>,
    pub added_at: BTreeMap<String, Option<String>>,
}

impl ListOrdering {
    fn empty(sort: ProfileSort) -> Value {
        json!({ "sort": sort.as_str(), "order": [], "addedAt": {} })
    }

    pub fn to_json(&self) -> Value {
        json!({ "sort": self.sort.as_str(), "order": self.order, "addedAt": self.added_at })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProfileOrdering {
    pub providers: ListOrdering,
    pub models: BTreeMap<String, ListOrdering>,
}

impl ProfileOrdering {
    pub fn to_json(&self) -> Value {
        json!({
            "providers": self.providers.to_json(),
            "models": self.models.iter().map(|(id, list)| (id.clone(), list.to_json())).collect::<Map<_, _>>(),
        })
    }
}

#[derive(Clone, Copy)]
pub enum ProfileList<'a> {
    Providers,
    Models(&'a str),
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(format!("provider library ordering: {}", message.into()))
}

fn parse_list(value: &Value) -> Result<ListOrdering> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("list must be an object"))?;
    let sort = object
        .get("sort")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("sort must be a string"))?;
    let order = object
        .get("order")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("order must be an array"))?;
    let order = order
        .iter()
        .map(|id| {
            id.as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| invalid("order must contain nonempty string IDs"))
        })
        .collect::<Result<Vec<_>>>()?;
    if order.iter().collect::<BTreeSet<_>>().len() != order.len() {
        return Err(invalid("order must not contain duplicate IDs"));
    }
    let dates = object
        .get("addedAt")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("addedAt must be an object"))?;
    let mut added_at = BTreeMap::new();
    for (id, value) in dates {
        let date = match value {
            Value::Null => None,
            Value::String(date) if DateTime::parse_from_rfc3339(date).is_ok() => Some(date.clone()),
            _ => {
                return Err(invalid(format!(
                    "addedAt for '{id}' must be an RFC 3339 timestamp or null"
                )))
            }
        };
        added_at.insert(id.clone(), date);
    }
    Ok(ListOrdering {
        sort: ProfileSort::parse(sort)?,
        order,
        added_at,
    })
}

pub(super) fn read(library: &Value) -> Result<ProfileOrdering> {
    let root = library
        .get(FIELD)
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("metadata must be an object"))?;
    let providers = root
        .get("providers")
        .ok_or_else(|| invalid("providers list is required"))?;
    let models = root
        .get("models")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("models must be an object"))?;
    Ok(ProfileOrdering {
        providers: parse_list(providers)?,
        models: models
            .iter()
            .map(|(id, value)| Ok((id.clone(), parse_list(value)?)))
            .collect::<Result<_>>()?,
    })
}

pub(super) fn validate(library: &Value) -> Result<()> {
    if library.get(FIELD).is_some() {
        read(library)?;
    }
    Ok(())
}

fn collections(library: &Value) -> Result<BTreeMap<String, Vec<String>>> {
    providers_object(library)?
        .iter()
        .map(|(id, provider)| {
            let ids = provider_view(id, provider)?
                .models
                .into_iter()
                .map(|model| model.id)
                .collect();
            Ok((id.clone(), ids))
        })
        .collect()
}

fn sync_list(list: &mut Value, ids: &[String], added_at: Option<&str>) {
    let current: BTreeSet<_> = ids.iter().map(String::as_str).collect();
    let order = list["order"].as_array_mut().expect("validated order");
    order.retain(|id| current.contains(id.as_str().expect("validated ID")));
    let known: BTreeSet<_> = order
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect();
    order.extend(
        ids.iter()
            .filter(|id| !known.contains(*id))
            .cloned()
            .map(Value::String),
    );
    let dates = list["addedAt"].as_object_mut().expect("validated dates");
    dates.retain(|id, _| current.contains(id.as_str()));
    for id in ids {
        dates
            .entry(id.clone())
            .or_insert_with(|| added_at.map_or(Value::Null, |date| json!(date)));
    }
}

// Hydrate legacy/external entries with unknown dates before editing. Only IDs
// introduced by an explicit operation receive its commit time afterwards.
pub(super) fn sync(library: &mut Value, added_at: Option<&str>) -> Result<()> {
    let lists = collections(library)?;
    if library.get(FIELD).is_none() {
        library[FIELD] =
            json!({ "providers": ListOrdering::empty(ProfileSort::NameAsc), "models": {} });
    }
    validate(library)?;
    let mut ids: Vec<_> = lists.keys().cloned().collect();
    ids.sort_by_key(|id| id.to_lowercase());
    sync_list(&mut library[FIELD]["providers"], &ids, added_at);
    let models = library[FIELD]["models"]
        .as_object_mut()
        .expect("validated model lists");
    models.retain(|id, _| lists.contains_key(id));
    for (id, ids) in lists {
        let list = models
            .entry(id)
            .or_insert_with(|| ListOrdering::empty(ProfileSort::Custom));
        sync_list(list, &ids, added_at);
    }
    Ok(())
}

pub(super) fn record_changes(library: &mut Value) -> Result<()> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    sync(library, Some(&now))
}

fn rename_in_list(list: &mut Value, old: &str, new: &str) {
    for id in list["order"].as_array_mut().expect("validated order") {
        if id.as_str() == Some(old) {
            *id = json!(new);
        }
    }
    let dates = list["addedAt"].as_object_mut().expect("validated dates");
    if let Some(date) = dates.remove(old) {
        dates.insert(new.into(), date);
    }
}

pub(super) fn rename_provider(library: &mut Value, old: &str, new: &str) {
    rename_in_list(&mut library[FIELD]["providers"], old, new);
    let models = library[FIELD]["models"]
        .as_object_mut()
        .expect("initialized model lists");
    if let Some(list) = models.remove(old) {
        models.insert(new.into(), list);
    }
}

pub(super) fn rename_model(library: &mut Value, provider: &str, old: &str, new: &str) {
    rename_in_list(&mut library[FIELD]["models"][provider], old, new);
}

fn list_mut<'a>(library: &'a mut Value, list: ProfileList<'_>) -> Result<&'a mut Value> {
    match list {
        ProfileList::Providers => Ok(&mut library[FIELD]["providers"]),
        ProfileList::Models(provider) => library[FIELD]["models"]
            .get_mut(provider)
            .ok_or_else(|| invalid(format!("provider '{provider}' no longer exists"))),
    }
}

pub fn set_profile_sort(paths: &Paths, list: ProfileList<'_>, sort: ProfileSort) -> Result<()> {
    let (lock, mut library, _) = lock_provider_documents(paths)?;
    list_mut(&mut library, list)?["sort"] = json!(sort.as_str());
    write_provider_changes(paths, &lock, None, None, &library)
}

pub fn reorder_profiles(paths: &Paths, list: ProfileList<'_>, ids: &[String]) -> Result<()> {
    let (lock, mut library, _) = lock_provider_documents(paths)?;
    let target = list_mut(&mut library, list)?;
    let current = parse_list(target)?.order;
    let requested: BTreeSet<_> = ids.iter().collect();
    if requested.len() != ids.len() || requested != current.iter().collect() {
        return Err(invalid(
            "the list changed; reload and provide every current ID exactly once",
        ));
    }
    target["order"] = json!(ids);
    target["sort"] = json!(ProfileSort::Custom.as_str());
    write_provider_changes(paths, &lock, None, None, &library)
}
