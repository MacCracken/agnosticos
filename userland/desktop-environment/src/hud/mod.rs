//! HUD widgets for the aethersafha desktop environment.
//!
//! Each widget in this module follows the same pattern:
//! - A state struct protected by `Arc<RwLock<_>>` so it can be cloned and
//!   shared across threads.
//! - A `render()` method that returns a plain data struct for the compositor
//!   (no pixel operations occur here; rendering is trait-driven upstream).
//! - An `update()` async method that fetches fresh data from daimon's HTTP
//!   or MCP APIs.
//! - A `start_polling()` helper that spawns a tokio task to call `update()`
//!   on a fixed interval.
//!
//! # Widgets
//! | Module | Widget | Data source |
//! |---|---|---|
//! | [`crew_status`] | [`crew_status::CrewStatusWidget`] | `agnostic_list_crews` MCP tool |
//! | [`domain_filter`] | [`domain_filter::DomainFilterWidget`] | `/v1/agents` REST API |
//! | [`gpu_status`] | [`gpu_status::GpuStatusWidget`] | `agnos_gpu_status` MCP tool |

pub mod crew_status;
pub mod domain_filter;
pub mod gpu_status;

pub use crew_status::{CrewEntry, CrewRunStatus, CrewStatusRenderData, CrewStatusWidget};
pub use domain_filter::{
    DomainAgentEntry, DomainFilterRenderData, DomainFilterWidget, DomainGroup,
};
pub use gpu_status::{GpuDeviceState, GpuStatusRenderData, GpuStatusWidget, MetricBand};
