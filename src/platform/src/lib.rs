#![feature(let_chains)]
extern crate core;

pub mod accounts;
pub mod auth;
pub mod context;
pub mod custom_events;
pub mod dashboards;
// pub mod datatype;
pub mod error;
pub mod event_records;
pub mod events;
mod group_records;
pub mod http;
pub mod projects;
pub mod properties;
pub mod queries;
pub mod reports;
// pub mod stub;

use std::fmt::Debug;
use std::sync::Arc;

use arrow::array::Array;
use arrow::array::ArrayRef;
use arrow::array::BooleanArray;
use arrow::array::Decimal128Array;
use arrow::array::Float32Array;
use arrow::array::Float64Array;
use arrow::array::Int16Array;
use arrow::array::Int32Array;
use arrow::array::Int64Array;
use arrow::array::Int8Array;
use arrow::array::StringArray;
use arrow::array::TimestampNanosecondArray;
use arrow::array::UInt16Array;
use arrow::array::UInt32Array;
use arrow::array::UInt64Array;
use arrow::array::UInt8Array;
use arrow::datatypes::TimeUnit;
use common::config::Config;
use common::types::DType;
use common::DECIMAL_PRECISION;
use common::DECIMAL_SCALE;
pub use context::Context;
use convert_case::Case;
use convert_case::Casing;
use datafusion_common::ScalarValue;
pub use error::PlatformError;
pub use error::Result;
use metadata::MetadataProvider;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Number;
use serde_json::Value;

use crate::accounts::Accounts;
use crate::auth::Auth;
use crate::custom_events::CustomEvents;
use crate::dashboards::Dashboards;
use crate::events::Events;
use crate::projects::Projects;
use crate::properties::Properties;
use crate::queries::Queries;
use crate::reports::Reports;

pub struct PlatformProvider {
    pub events: Arc<Events>,
    pub custom_events: Arc<CustomEvents>,
    pub event_properties: Arc<Properties>,
    pub user_properties: Arc<Properties>,
    pub system_properties: Arc<Properties>,
    pub accounts: Arc<Accounts>,
    pub auth: Arc<Auth>,
    pub query: Arc<Queries>,
    pub dashboards: Arc<Dashboards>,
    pub reports: Arc<Reports>,
    pub projects: Arc<Projects>, /* pub event_records: Arc<dyn event_records::Provider>,
                                  * pub group_records: Arc<dyn group_records::Provider>, */
}

impl PlatformProvider {
    pub fn new(
        md: Arc<MetadataProvider>,
        query_prov: Arc<query::QueryProvider>,
        cfg: Config,
    ) -> Self {
        Self {
            events: Arc::new(Events::new(md.events.clone())),
            custom_events: Arc::new(CustomEvents::new(md.custom_events.clone())),
            event_properties: Arc::new(Properties::new_event(md.event_properties.clone())),
            user_properties: Arc::new(Properties::new_user(md.user_properties.clone())),
            system_properties: Arc::new(Properties::new_user(md.system_properties.clone())),
            accounts: Arc::new(Accounts::new(md.accounts.clone())),
            auth: Arc::new(Auth::new(md.accounts.clone(), cfg.clone())),
            query: Arc::new(Queries::new(query_prov)),
            dashboards: Arc::new(Dashboards::new(md.dashboards.clone())),
            reports: Arc::new(Reports::new(md.reports.clone())),
            // event_records: Arc::new(stub::EventRecords {}),
            // group_records: Arc::new(stub::GroupRecords {}),
            projects: Arc::new(Projects::new(md.clone(), cfg)),
        }
    }
}

#[macro_export]
macro_rules! arr_to_json_values {
    ($array_ref:expr,$array_type:ident) => {{
        let arr = $array_ref.as_any().downcast_ref::<$array_type>().unwrap();
        Ok(arr.iter().map(|value| json!(value)).collect())
    }};
}

pub fn array_ref_to_json_values(arr: &ArrayRef) -> Result<Vec<Value>> {
    match arr.data_type() {
        arrow::datatypes::DataType::Int8 => arr_to_json_values!(arr, Int8Array),
        arrow::datatypes::DataType::Int16 => arr_to_json_values!(arr, Int16Array),
        arrow::datatypes::DataType::Int32 => arr_to_json_values!(arr, Int32Array),
        arrow::datatypes::DataType::Int64 => arr_to_json_values!(arr, Int64Array),
        arrow::datatypes::DataType::UInt8 => arr_to_json_values!(arr, UInt8Array),
        arrow::datatypes::DataType::UInt16 => arr_to_json_values!(arr, UInt16Array),
        arrow::datatypes::DataType::UInt32 => arr_to_json_values!(arr, UInt32Array),
        arrow::datatypes::DataType::UInt64 => arr_to_json_values!(arr, UInt64Array),
        arrow::datatypes::DataType::Float32 => {
            let arr = arr.as_any().downcast_ref::<Float32Array>().unwrap();
            Ok(arr
                .iter()
                .map(|value| {
                    // https://stackoverflow.com/questions/73871891/how-to-serialize-a-struct-containing-f32-using-serde-json
                    json!(value.map(|v| (v as f64 * 1000000.0).trunc() / 1000000.0))
                })
                .collect())

            // arr_to_json_values!(arr, Float32Array)
        }
        arrow::datatypes::DataType::Float64 => arr_to_json_values!(arr, Float64Array),
        arrow::datatypes::DataType::Boolean => arr_to_json_values!(arr, BooleanArray),
        arrow::datatypes::DataType::Utf8 => arr_to_json_values!(arr, StringArray),
        arrow::datatypes::DataType::Timestamp(TimeUnit::Nanosecond, _) => {
            arr_to_json_values!(arr, TimestampNanosecondArray)
        }
        arrow::datatypes::DataType::Decimal128(_, _s) => {
            let arr = arr.as_any().downcast_ref::<Decimal128Array>().unwrap();
            arr.iter()
                .map(|value| match value {
                    None => Ok(Value::Null),
                    Some(v) => {
                        let d = Decimal::from_i128_with_scale(v, DECIMAL_SCALE as u32);
                        let d_f = match d.to_f64() {
                            None => {
                                return Err(PlatformError::Internal(
                                    "can't convert decimal to f64".to_string(),
                                ));
                            }
                            Some(v) => v,
                        };
                        let n = match Number::from_f64(d_f) {
                            None => {
                                return Err(PlatformError::Internal(
                                    "can't make json number from f64".to_string(),
                                ));
                            }
                            Some(v) => v,
                        };
                        Ok(Value::Number(n))
                    }
                })
                .collect::<Result<_>>()
        }
        _ => unimplemented!("{}", arr.data_type()),
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResponseMetadata {
    pub next: Option<String>,
}

impl From<metadata::metadata::ResponseMetadata> for ResponseMetadata {
    fn from(value: metadata::metadata::ResponseMetadata) -> Self {
        ResponseMetadata { next: value.next }
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ListResponse<T>
where T: Debug
{
    pub data: Vec<T>,
    pub meta: ResponseMetadata,
}

impl<A, B> TryInto<ListResponse<A>> for metadata::metadata::ListResponse<B>
where
    A: Debug,
    B: TryInto<A> + Clone + Debug,
    PlatformError: std::convert::From<<B as TryInto<A>>::Error>,
{
    type Error = PlatformError;

    fn try_into(self) -> std::result::Result<ListResponse<A>, Self::Error> {
        let data = self
            .data
            .into_iter()
            .map(|v| v.try_into())
            .collect::<std::result::Result<Vec<A>, B::Error>>()?;
        let meta = ResponseMetadata {
            next: self.meta.next,
        };
        Ok(ListResponse { data, meta })
    }
}

pub fn json_value_to_scalar(v: &Value) -> Result<ScalarValue> {
    match v {
        Value::Bool(v) => Ok(ScalarValue::Boolean(Some(*v))),
        Value::Number(n) => {
            let dec = Decimal::try_from(n.as_f64().unwrap())?;
            Ok(ScalarValue::Decimal128(
                Some(dec.mantissa()),
                DECIMAL_PRECISION,
                dec.scale() as i8,
            ))
        }
        Value::String(v) => Ok(ScalarValue::Utf8(Some(v.to_string()))),
        _ => Err(PlatformError::BadRequest("unexpected value".to_string())),
    }
}

pub fn scalar_to_json_value(v: &ScalarValue) -> Result<Value> {
    match v {
        ScalarValue::Decimal128(None, _, _) => Ok(Value::Null),
        ScalarValue::Boolean(None) => Ok(Value::Null),
        ScalarValue::Utf8(None) => Ok(Value::Null),
        ScalarValue::Decimal128(Some(v), _p, s) => Ok(Value::Number(
            Number::from_f64(Decimal::new(*v as i64, *s as u32).to_f64().unwrap()).unwrap(),
        )),
        ScalarValue::Boolean(Some(v)) => Ok(Value::Bool(*v)),
        ScalarValue::Utf8(Some(v)) => Ok(Value::String(v.to_owned())),
        _ => Err(PlatformError::BadRequest("unexpected value".to_string())),
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PropValueOperation {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    True,
    False,
    Exists,
    Empty,
    Regex,
    Like,
    NotLike,
    NotRegex,
}

impl TryInto<common::query::PropValueOperation> for PropValueOperation {
    type Error = PlatformError;

    fn try_into(self) -> std::result::Result<common::query::PropValueOperation, Self::Error> {
        Ok(match self {
            PropValueOperation::Eq => common::query::PropValueOperation::Eq,
            PropValueOperation::Neq => common::query::PropValueOperation::Neq,
            PropValueOperation::Gt => common::query::PropValueOperation::Gt,
            PropValueOperation::Gte => common::query::PropValueOperation::Gte,
            PropValueOperation::Lt => common::query::PropValueOperation::Lt,
            PropValueOperation::Lte => common::query::PropValueOperation::Lte,
            PropValueOperation::True => common::query::PropValueOperation::True,
            PropValueOperation::False => common::query::PropValueOperation::False,
            PropValueOperation::Exists => common::query::PropValueOperation::Exists,
            PropValueOperation::Empty => common::query::PropValueOperation::Empty,
            PropValueOperation::Regex => common::query::PropValueOperation::Regex,
            PropValueOperation::Like => common::query::PropValueOperation::Like,
            PropValueOperation::NotLike => common::query::PropValueOperation::NotLike,
            PropValueOperation::NotRegex => common::query::PropValueOperation::NotRegex,
        })
    }
}

impl TryInto<PropValueOperation> for common::query::PropValueOperation {
    type Error = PlatformError;

    fn try_into(self) -> std::result::Result<PropValueOperation, Self::Error> {
        Ok(match self {
            common::query::PropValueOperation::Eq => PropValueOperation::Eq,
            common::query::PropValueOperation::Neq => PropValueOperation::Neq,
            common::query::PropValueOperation::Gt => PropValueOperation::Gt,
            common::query::PropValueOperation::Gte => PropValueOperation::Gte,
            common::query::PropValueOperation::Lt => PropValueOperation::Lt,
            common::query::PropValueOperation::Lte => PropValueOperation::Lte,
            common::query::PropValueOperation::True => PropValueOperation::True,
            common::query::PropValueOperation::False => PropValueOperation::False,
            common::query::PropValueOperation::Exists => PropValueOperation::Exists,
            common::query::PropValueOperation::Empty => PropValueOperation::Empty,
            common::query::PropValueOperation::Regex => PropValueOperation::Regex,
            common::query::PropValueOperation::Like => PropValueOperation::Like,
            common::query::PropValueOperation::NotLike => PropValueOperation::NotLike,
            common::query::PropValueOperation::NotRegex => PropValueOperation::NotRegex,
        })
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "eventType", rename_all = "camelCase")]
pub enum EventRef {
    #[serde(rename_all = "camelCase")]
    Regular { event_name: String },
    #[serde(rename_all = "camelCase")]
    Custom { event_id: u64 },
}

impl EventRef {
    pub fn name(&self, idx: usize) -> String {
        match self {
            EventRef::Regular { event_name } => {
                format!("{}_regular_{}", event_name.to_case(Case::Snake), idx)
            }
            EventRef::Custom { event_id } => format!("{event_id}_custom_{idx}"),
        }
    }
}

impl From<EventRef> for common::query::EventRef {
    fn from(e: EventRef) -> Self {
        match e {
            EventRef::Regular { event_name } => common::query::EventRef::RegularName(event_name),
            EventRef::Custom { event_id } => common::query::EventRef::Custom(event_id),
        }
    }
}

impl TryInto<EventRef> for common::query::EventRef {
    type Error = PlatformError;

    fn try_into(self) -> std::result::Result<EventRef, Self::Error> {
        Ok(match self {
            common::query::EventRef::RegularName(name) => EventRef::Regular { event_name: name },
            common::query::EventRef::Regular(_id) => unimplemented!(),
            common::query::EventRef::Custom(id) => EventRef::Custom { event_id: id },
        })
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "propertyType", rename_all = "camelCase")]
pub enum PropertyRef {
    #[serde(rename_all = "camelCase")]
    System { property_name: String },
    #[serde(rename_all = "camelCase")]
    User { property_name: String },
    #[serde(rename_all = "camelCase")]
    Event { property_name: String },
    #[serde(rename_all = "camelCase")]
    Custom { property_id: u64 },
}

impl TryInto<common::query::PropertyRef> for PropertyRef {
    type Error = PlatformError;

    fn try_into(self) -> std::result::Result<common::query::PropertyRef, Self::Error> {
        Ok(match self {
            PropertyRef::System { property_name } => {
                common::query::PropertyRef::System(property_name)
            }
            PropertyRef::User { property_name } => common::query::PropertyRef::User(property_name),
            PropertyRef::Event { property_name } => {
                common::query::PropertyRef::Event(property_name)
            }
            PropertyRef::Custom { property_id } => common::query::PropertyRef::Custom(property_id),
        })
    }
}

impl TryInto<PropertyRef> for common::query::PropertyRef {
    type Error = PlatformError;

    fn try_into(self) -> std::result::Result<PropertyRef, Self::Error> {
        Ok(match self {
            common::query::PropertyRef::System(property_name) => {
                PropertyRef::System { property_name }
            }
            common::query::PropertyRef::User(property_name) => PropertyRef::User { property_name },
            common::query::PropertyRef::Event(property_name) => {
                PropertyRef::Event { property_name }
            }
            common::query::PropertyRef::Custom(property_id) => PropertyRef::Custom { property_id },
        })
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EventFilter {
    #[serde(rename_all = "camelCase")]
    Property {
        #[serde(flatten)]
        property: PropertyRef,
        operation: PropValueOperation,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<Vec<Value>>,
    },
    #[serde(rename_all = "camelCase")]
    Cohort { cohort_id: u64 },
    #[serde(rename_all = "camelCase")]
    Group { group_id: u64 },
}

impl TryInto<common::query::EventFilter> for &EventFilter {
    type Error = PlatformError;

    fn try_into(self) -> std::result::Result<common::query::EventFilter, Self::Error> {
        Ok(match self {
            EventFilter::Property {
                property,
                operation,
                value,
            } => common::query::EventFilter::Property {
                property: property.to_owned().try_into()?,
                operation: operation.to_owned().try_into()?,
                value: match value {
                    None => None,
                    Some(v) => {
                        if v.is_empty() {
                            None
                        } else {
                            Some(v.iter().map(json_value_to_scalar).collect::<Result<_>>()?)
                        }
                    }
                },
            },
            _ => todo!(),
        })
    }
}

impl TryInto<EventFilter> for &common::query::EventFilter {
    type Error = PlatformError;

    fn try_into(self) -> std::result::Result<EventFilter, Self::Error> {
        Ok(match self {
            common::query::EventFilter::Property {
                property,
                operation,
                value,
            } => EventFilter::Property {
                property: property.to_owned().try_into()?,
                operation: operation.to_owned().try_into()?,
                value: match value {
                    None => None,
                    Some(v) => Some(v.iter().map(scalar_to_json_value).collect::<Result<_>>()?),
                },
            },
        })
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ColumnType {
    Dimension,
    Metric,
    MetricValue,
    FunnelMetricValue,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    #[serde(rename = "type")]
    pub typ: ColumnType,
    pub name: String,
    pub is_nullable: bool,
    pub data_type: DType,
    pub step: Option<usize>,
    pub data: Vec<Value>,
    pub compare_values: Option<Vec<Value>>,
}

impl From<query::ColumnType> for ColumnType {
    fn from(value: query::ColumnType) -> Self {
        match value {
            query::ColumnType::Dimension => ColumnType::Dimension,
            query::ColumnType::Metric => ColumnType::Metric,
            query::ColumnType::MetricValue => ColumnType::MetricValue,
            query::ColumnType::FunnelMetricValue => ColumnType::FunnelMetricValue,
        }
    }
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JSONQueryResponse {
    columns: Vec<Column>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JSONCompactQueryResponse(Vec<Vec<Value>>);

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum QueryResponse {
    JSON(JSONQueryResponse),
    JSONCompact(JSONCompactQueryResponse),
}

impl QueryResponse {
    pub fn try_new_json(columns: Vec<query::Column>) -> Result<Self> {
        let columns = columns
            .iter()
            .cloned()
            .map(|column| match array_ref_to_json_values(&column.data) {
                Ok(data) => Ok(Column {
                    typ: column.typ.into(),
                    name: column.name,
                    is_nullable: column.is_nullable,
                    data_type: column.data_type.try_into()?,
                    data,
                    step: None,
                    compare_values: None,
                }),
                Err(err) => Err(err),
            })
            .collect::<Result<_>>()?;

        Ok(Self::JSON(JSONQueryResponse { columns }))
    }

    pub fn try_new_json_compact(columns: Vec<query::Column>) -> Result<Self> {
        let data = columns
            .iter()
            .cloned()
            .map(|column| match array_ref_to_json_values(&column.data) {
                Ok(data) => Ok(data),
                Err(err) => Err(err),
            })
            .collect::<Result<_>>()?;

        Ok(Self::JSONCompact(JSONCompactQueryResponse(data)))
    }
}

#[derive(Clone, Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EventGroupedFiltersGroupsCondition {
    And,
    #[default]
    Or,
}

#[derive(Clone, Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EventGroupedFilterGroupCondition {
    #[default]
    And,
    Or,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EventGroupedFilterGroup {
    #[serde(default)]
    pub filters_condition: EventGroupedFilterGroupCondition,
    pub filters: Vec<EventFilter>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EventGroupedFilters {
    #[serde(default)]
    pub groups_condition: Option<EventGroupedFiltersGroupsCondition>,
    pub groups: Vec<EventGroupedFilterGroup>,
}
